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
        let body = measure!("json", { serde_json::from_str(&content).unwrap() });
        return Ok(Json(DiscoveryResponse::new(body)));
    } else {
        return Err(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("No configuration found for this Envoy version + resource type".to_string())
            .unwrap());
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    let (tx, rx) = watch::channel(json!([]));
    tokio::spawn(async move {
        loop {
            for source in settings.sources.iter() {
                let val = source.get();
                match val {
                    Ok(s) => {
                        measure!("polling", {
                            if let Ok(json_value) = serde_json::from_str(&s) {
                                let _ = tx.send(json_value);
                            }
                        })
                    }
                    Err(e) => println!("Failed to get from source: {e}"),
                }
            }
            sleep(Duration::from_secs(30)).await;
        }
    });

    let mut env = Environment::new();

    for template in settings.templates.iter() {
        env.add_template_owned(template.name(), template.source()?)?;
        println!("Added template: {}", template.name());
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
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
