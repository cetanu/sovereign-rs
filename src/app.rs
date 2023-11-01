use crate::context::DeserializeAs;
use crate::envoy_types::DiscoveryRequest;
use crate::sources::{InstancesPackage, SourceDest};
use crate::templates::XdsTemplate;
use axum::body::{Bytes, Full};
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dashmap::DashMap;
use minijinja::{context, Environment, Value as JinjaValue};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::sync::watch::Receiver;
use tracing::info;

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

pub struct State<'a> {
    pub instances: Option<Receiver<Vec<InstancesPackage>>>,
    pub context: Option<Receiver<JinjaValue>>,
    pub templates: DashMap<String, XdsTemplate>,
    pub env: Environment<'a>,
}

impl<'a> State<'a> {
    fn template(&'a self, envoy_version: &str, resource_type: &str) -> Option<XdsTemplate> {
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

pub async fn discovery(
    Path((api_version, resource)): Path<(String, String)>,
    Json(payload): Json<DiscoveryRequest>,
    Extension(state): Extension<Arc<State<'_>>>,
) -> Result<Response<Full<Bytes>>, impl IntoResponse> {
    let (_, resource_type) = resource.split_once(':').unwrap();
    let version = measure!("envoy version", { payload.envoy_version() });
    let templ = measure!("template", { state.template(&version, resource_type) });
    let service_cluster = payload.cluster();

    info!(
        api_version = %api_version,
        resource_type = %resource_type,
        version = %version,
        service_cluster = %service_cluster
    );

    if let Some(template) = templ {
        let mut i = json! {[]};
        let borrow = i.as_array_mut().unwrap();

        if let Some(sources) = &state.instances {
            let instances = measure!("sources", { sources.borrow().clone() });
            measure!("filtering", {
                instances
                    .into_iter()
                    .filter(|instance| match &instance.dest {
                        SourceDest::Match(val) => val == service_cluster,
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

        let text = measure!(
            "render",
            match template.call_python {
                Some(true) => template
                    .call(context! {
                            instances => i,
                            discovery_request => payload,
                            ..ctx
                    })
                    .unwrap(),
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

        let hash = measure!("hashing", xxhash_rust::xxh64::xxh64(text.as_bytes(), 0));
        if hash.to_string() == payload.version_info {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Full::from(""))
                .unwrap());
        }

        let response = measure!(
            "deser",
            match template.deserialize_as {
                DeserializeAs::Yaml => {
                    let y: JsonValue = match serde_yaml::from_str(&text) {
                        Ok(yombl) => yombl,
                        Err(e) => {
                            intercept_yaml_error(&e.to_string(), &text);
                            panic!("{e}");
                        }
                    };
                    let res = serde_json::to_string(&y).unwrap();
                    format!("{{\"version_info\": \"{hash}\", \"resources\": {res}}}")
                }
                // JSON / Plaintext are chucked straight in
                _ => {
                    format!("{{\"version_info\": \"{hash}\", \"resources\": {text}}}")
                }
            }
        );

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Full::from(response))
            .unwrap())
    } else {
        Err(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(format!(
                "No configuration found for {resource_type}:{version}. Full list: {:?}",
                state
                    .templates
                    .iter()
                    .map(|i| i.key().to_string())
                    .collect::<Vec<String>>()
            ))
            .unwrap())
    }
}

fn intercept_yaml_error(message: &str, content: &str) {
    if let Some(line_start) = message.find("line ") {
        if let Some(column_start) = message.find("column ") {
            let line_str = &message[line_start + 5..column_start - 1];
            let column_str = &message[column_start + 7..];

            let line: usize = line_str.parse().unwrap();
            let column: usize = column_str.parse().unwrap();

            let start = if line >= 5 { line - 5 } else { 1 };
            let end = line + 5;

            let lines = content
                .split('\n')
                .enumerate()
                // Start index from 1
                .map(|(i, txt)| (i + 1, txt));

            for (idx, text) in lines {
                if idx >= start && idx <= end {
                    println!("{}: {}", idx, text);
                    if idx == line {
                        println!("{}^", " ".repeat(column + 3));
                    }
                }
            }
        }
    }
}
