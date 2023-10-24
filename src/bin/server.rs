use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use minijinja::{context, Environment, Template, Value as JinjaValue};
use serde_json::json;
use sovereign_rs::context::poll_context;
use sovereign_rs::sources::{
    poll_sources, poll_sources_into_buckets, InstancesPackage, SourceDest,
};
use tokio::sync::watch::{self, Receiver, Sender};
use tokio::time::{sleep, Duration};

use sovereign_rs::config::{Settings, SourceConfig, TemplateContextConfig};
use sovereign_rs::types::{DiscoveryRequest, DiscoveryResponse};

struct State<'a> {
    instances: Option<Receiver<Vec<InstancesPackage>>>,
    context: Option<Receiver<JinjaValue>>,
    env: Environment<'a>,
}

impl<'a> State<'a> {
    // TODO: figure out:
    // 1. How to choose json OR yaml
    // 2. How to choose maybe a python script, which passes in context to a method
    fn template(&'a self, envoy_version: String, resource_type: &str) -> Option<Template<'a, 'a>> {
        // Incrementally walk the semantic version to find a template
        let mut octets = envoy_version.split('.').collect::<Vec<_>>();
        while !octets.is_empty() {
            let prefix = octets.join(".");
            let name = format!("{}/{}", prefix, resource_type);
            if let Ok(template) = self.env.get_template(&name) {
                return Some(template);
            }
            octets.pop();
        }
        // Try the default template
        let name = format!("default/{}", resource_type);
        if let Ok(template) = self.env.get_template(&name) {
            return Some(template);
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
        println!("{}: Time elapsed: {:?}", $name, duration);
        result
    }};
}

async fn discovery(
    Path((_api_version, resource)): Path<(String, String)>,
    Json(payload): Json<DiscoveryRequest>,
    Extension(state): Extension<Arc<State<'_>>>,
) -> Result<Json<DiscoveryResponse>, impl IntoResponse> {
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
        let content = measure!("render", {
            let result = template.render(context! {
                instances => i,
                discovery_request => payload,
                ..ctx.clone()
            });
            match result {
                Ok(text) => text,
                Err(e) => {
                    println!("{e}");
                    return Err(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(format!("{ctx}"))
                        .unwrap());
                }
            }
        });

        let deser = measure!("json", { serde_json::from_str(&content) });
        match deser {
            Ok(body) => {
                let response = measure!("hash", { DiscoveryResponse::new(body) });
                Ok(Json(response))
            }
            Err(e) => {
                println!("Failed to deserialize content:");
                for (idx, line) in content.split('\n').enumerate() {
                    println!("{idx}: {line}");
                }
                panic!("{e}")
            }
        }
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

    let mut env = Environment::new();
    for template in settings.templates.iter() {
        let s = format!("Added template: {}", template.name());
        measure!(s.as_str(), {
            env.add_template_owned(template.name(), template.source()?)?
        });
    }

    let state = Arc::new(State {
        instances: sources_rx,
        context: context_rx,
        env,
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
