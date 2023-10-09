use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use dashmap::DashMap;
use minijinja::{context, Environment, Template, Value as JinjaValue};
use serde_json::Value as JsonValue;
use sovereign_rs::context::{poll_context, Parsed};
use sovereign_rs::sources::poll_sources;
use tokio::sync::watch::{self, Receiver};
use tokio::time::{sleep, Duration};

use sovereign_rs::config::Settings;
use sovereign_rs::types::{DiscoveryRequest, DiscoveryResponse};

struct State<'a> {
    _shared: DashMap<String, String>,
    instances: Receiver<JsonValue>,
    context: Receiver<JinjaValue>,
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
    let (_, resource_type) = resource.split_once(":").unwrap();
    let version = measure!("envoy version", { payload.envoy_version() });
    let templ = measure!("template", { state.template(version, resource_type) });

    if let Some(template) = templ {
        let instances = measure!("sources", { state.instances.borrow().clone() });

        let ctx = state.context.borrow().clone();

        let content = measure!("render", {
            template
                .render(context! {
                    instances => instances,
                    discovery_request => payload,
                    ..ctx
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };

    let (sources_tx, sources_rx) = watch::channel(poll_sources(&settings.sources));
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            let sources = poll_sources(&settings.sources);
            _ = sources_tx.send(sources);
        }
    });

    let (context_tx, context_rx) = watch::channel(poll_context(&settings.template_context));
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            let ctx = poll_context(&settings.template_context);
            _ = context_tx.send(ctx);
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
        instances: sources_rx,
        context: context_rx,
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
