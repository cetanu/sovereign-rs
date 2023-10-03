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

struct State {
    a: DashMap<String, String>,
    ch: Receiver<JsonValue>,
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
    let (_tx, rx) = watch::channel(json!({"hello": "foo"}));
    let state = Arc::new(State {
        a: DashMap::new(),
        ch: rx,
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
