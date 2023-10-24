use axum::body::{Bytes, Full};
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use dashmap::DashMap;
use minijinja::{context, Environment, Value as JinjaValue};
use serde_json::{json, Value as JsonValue};
use sovereign_rs::config::{Settings, SourceConfig, TemplateContextConfig, XdsTemplate};
use sovereign_rs::context::{poll_context, DeserializeAs};
use sovereign_rs::sources::{
    poll_sources, poll_sources_into_buckets, InstancesPackage, SourceDest,
};
use sovereign_rs::types::DiscoveryRequest;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch::{self, Receiver, Sender};
use tokio::time::{sleep, Duration};

struct State<'a> {
    instances: Option<Receiver<Vec<InstancesPackage>>>,
    context: Option<Receiver<JinjaValue>>,
    templates: DashMap<String, XdsTemplate>,
    env: Environment<'a>,
}

impl<'a> State<'a> {
    fn template(&'a self, envoy_version: String, resource_type: &str) -> Option<XdsTemplate> {
        // Incrementally walk the semantic version to find a template
        let mut octets = envoy_version.split('.').collect::<Vec<_>>();
        while !octets.is_empty() {
            let prefix = octets.join(".");
            let name = format!("{}/{}", prefix, resource_type);
            if let Some(template) = self.templates.get(&name) {
                return Some(template.clone());
            }
            octets.pop();
        }
        // Try the default template
        let name = format!("default/{}", resource_type);
        if let Some(template) = self.templates.get(&name) {
            return Some(template.clone());
        }
        None
    }
}

#[macro_export]
macro_rules! measure {
    ($name:expr, $block:expr) => {{
        let start = std::time::Instant::now();
        let result = $block;
        let duration = start.elapsed();
        println!("{} ({:?})", $name, duration);
        result
    }};
}

async fn discovery(
    Path((_api_version, resource)): Path<(String, String)>,
    Json(payload): Json<DiscoveryRequest>,
    Extension(state): Extension<Arc<State<'_>>>,
) -> Result<Response<Full<Bytes>>, impl IntoResponse> {
    let (_, resource_type) = resource.split_once(':').unwrap();
    let version = measure!("envoy version", { payload.envoy_version() });
    let templ = measure!("template", { state.template(version, resource_type) });
    let node_key = payload.cluster();

    if let Some(template) = templ {
        let mut i = json! {[]};
        let borrow = i.as_array_mut().unwrap();

        if let Some(sources) = &state.instances {
            let instances = measure!("sources", { sources.borrow().clone() });
            measure!("filtering", {
                instances
                    .into_iter()
                    .filter(|instance| match &instance.dest {
                        SourceDest::Match(value) => value == node_key,
                        SourceDest::Any => true,
                    })
                    .for_each(|instance| {
                        if let Some(instances) = instance.instances.as_array() {
                            borrow.extend(instances.clone())
                        }
                    })
            });
        }

        let mut ctx = minijinja::context! {};
        if let Some(c) = &state.context {
            ctx = c.borrow().clone();
        }

        let content = measure!(
            "render",
            match template.call_python {
                Some(true) => template.call(context! {
                        instances => i,
                        discovery_request => payload,
                        ..ctx
                }),
                _ => {
                    let template_string = template.source().unwrap();
                    let result = state.env.render_str(
                        template_string.as_str(),
                        context! {
                            instances => i,
                            discovery_request => payload,
                            ..ctx
                        },
                    );
                    match result {
                        Ok(text) => text,
                        Err(e) => {
                            return Err(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(format!("{e}"))
                                .unwrap());
                        }
                    }
                }
            }
        );

        let hash = measure!("hashing", xxhash_rust::xxh64::xxh64(content.as_bytes(), 0));
        if hash.to_string() == payload.version_info {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Full::from(""))
                .unwrap());
        }

        let body = measure!(
            "deser",
            match template.deserialize_as {
                DeserializeAs::Yaml => {
                    let y: JsonValue = serde_yaml::from_str(&content).unwrap();
                    let res = serde_json::to_string(&y).unwrap();
                    format!("{{\"version_info\": \"{hash}\", \"resources\": {res}}}")
                }
                // JSON / Plaintext are chucked straight in
                _ => {
                    format!("{{\"version_info\": \"{hash}\", \"resources\": {content}}}")
                }
            }
        );

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Full::from(body))
            .unwrap())
    } else {
        Err(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("No configuration found for this Envoy version + resource type".to_string())
            .unwrap())
    }
}

async fn setup_context_channel(config: TemplateContextConfig) -> Receiver<JinjaValue> {
    let (context_tx, context_rx) = watch::channel(poll_context(&config.items));
    tokio::spawn(async move {
        loop {
            sleep(config.interval).await;
            let ctx = poll_context(&config.items);
            _ = context_tx.send(ctx);
        }
    });
    context_rx
}

fn setup_sources_channel(
    settings: Settings,
    config: SourceConfig,
) -> Receiver<Vec<InstancesPackage>> {
    let (sources_tx, sources_rx): (
        Sender<Vec<InstancesPackage>>,
        Receiver<Vec<InstancesPackage>>,
    );
    if let Some(matching) = settings.node_matching {
        let instances = poll_sources_into_buckets(&config.items, &matching.source_key).unwrap();
        (sources_tx, sources_rx) = watch::channel(instances);
        tokio::spawn(async move {
            loop {
                sleep(config.interval).await;
                if let Ok(sources) = poll_sources_into_buckets(&config.items, &matching.source_key)
                {
                    _ = sources_tx.send(sources);
                }
            }
        });
    } else {
        let initial = poll_sources(&config.items).unwrap();
        (sources_tx, sources_rx) = watch::channel(initial);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                if let Ok(sources) = poll_sources(&config.items) {
                    _ = sources_tx.send(sources);
                }
            }
        });
    }
    sources_rx
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    let mut sources_rx = None;
    if let Some(source_conf) = &settings.sources {
        sources_rx = Some(setup_sources_channel(settings.clone(), source_conf.clone()));
    }

    let mut context_rx = None;
    if let Some(context_conf) = &settings.template_context {
        context_rx = Some(setup_context_channel(context_conf.clone()).await);
    }

    let templates = DashMap::new();
    for template in settings.templates.iter() {
        let s = format!("Added template: {}", template.name());
        measure!(s.as_str(), {
            templates.insert(template.name(), template.clone())
        });
    }

    let state = Arc::new(State {
        instances: sources_rx,
        context: context_rx,
        env: Environment::new(),
        templates,
    });

    let app = Router::new()
        .route("/:version/*resource", post(discovery))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8070));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
