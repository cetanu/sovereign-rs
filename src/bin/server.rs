use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use dashmap::DashMap;
use minijinja::{context, Environment, Template};
use serde_json::json;
use serde_json::Value as JsonValue;
use sovereign_rs::sources::Source;
use tokio::sync::watch::{self, Receiver};
use tokio::time::{sleep, Duration};

use sovereign_rs::config::Settings;
use sovereign_rs::types::{DiscoveryRequest, DiscoveryResponse};

struct State<'a> {
    _shared: DashMap<String, String>,
    instances: Receiver<JsonValue>,
    env: Environment<'a>,
}

impl<'a> State<'a> {
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
    let (_, resource_type) = resource.split_once(":").unwrap();
    let version = measure!("envoy version", { payload.envoy_version() });
    let templ = measure!("template", { state.template(version, resource_type) });

    if let Some(template) = templ {
        let instances = measure!("sources", { state.instances.borrow().clone() });
        let content = measure!("render", {
            template
                .render(context! {
                    instances => instances,
                    discovery_request => payload,
                })
                .unwrap()
        });
        let deser = measure!("json", { serde_json::from_str(&content) });
        match deser {
            Ok(body) => {
                let response = measure!("hash", { DiscoveryResponse::new(body) });
                Ok(Json(response))
            }
            Err(e) => {
                println!("Failed to deserialize content:");
                for (idx, line) in content.split("\n").into_iter().enumerate() {
                    println!("{idx}: {line}");
                }
                panic!("{e}")
            }
        }
    } else {
        return Err(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("No configuration found for this Envoy version + resource type".to_string())
            .unwrap());
    }
}

fn poll_sources(sources: &[Source]) -> JsonValue {
    let mut ret = json! {[]};
    for source in sources.iter() {
        let val = measure!("polling", { source.get() });
        if let Ok(s) = val {
            if let Ok(json_value) = serde_json::from_str::<JsonValue>(&s) {
                if let Some(s) = ret.as_array_mut() {
                    if let Some(instances) = json_value.as_array() {
                        s.extend(instances.clone());
                    }
                }
            }
        } else {
            panic!("Failed to get from source");
        }
    }
    ret
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    let (tx, rx) = watch::channel(poll_sources(&settings.sources));
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            let sources = poll_sources(&settings.sources);
            _ = tx.send(sources);
        }
    });

    let mut env = Environment::new();

    for template in settings.templates.iter() {
        let s = format!("Added template: {}", template.name());
        measure!(s.as_str(), {
            env.add_template_owned(template.name(), template.source()?)?
        });
    }

    let state = Arc::new(State {
        _shared: DashMap::new(),
        instances: rx,
        env,
    });

    let app = Router::new()
        .route("/:version/*resource", post(discovery))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8070));
    println!("Starting server");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
