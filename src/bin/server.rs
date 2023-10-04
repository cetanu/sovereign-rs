use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::routing::post;
use axum::{Json, Router};
use dashmap::DashMap;
use serde::Serialize;
use serde_json::json;
use serde_json::Value as JsonValue;

use tokio::sync::watch::{self, Receiver, Sender};

use sovereign_rs::config::Settings;

struct State {
    shared: DashMap<String, String>,
    context: Receiver<JsonValue>,
}

#[derive(Serialize)]
struct DiscoveryResponse {
    resources: Vec<JsonValue>,
    version_info: String,
}

async fn discovery(Path((version, resource)): Path<(String, String)>) -> Json<DiscoveryResponse> {
    let (_, resource_type) = resource.split_once(":").unwrap();
    println!("{version}, {resource_type}");
    return Json(DiscoveryResponse {
        resources: vec![],
        version_info: "0".to_string(),
    });
}

#[tokio::main]
async fn main() {
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            panic!("Could not load config: {e}")
        }
    };
    for source in settings.sources.iter() {
        let val = source.get();
        match val {
            Ok(s) => println!("{s}"),
            Err(e) => panic!("Failed to get from source: {e}"),
        }
    }

    let (_tx, rx) = watch::channel(json!({"hello": "foo"}));
    let state = Arc::new(State {
        shared: DashMap::new(),
        context: rx,
    });

    let app = Router::new()
        .route("/:version/*resource", post(discovery))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8070));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
