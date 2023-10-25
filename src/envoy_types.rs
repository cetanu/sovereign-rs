use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use xxhash_rust::xxh64::xxh64;

#[derive(Serialize)]
pub struct DiscoveryResponse {
    resources: JsonValue,
    version_info: String,
}

impl DiscoveryResponse {
    pub fn new(resources: JsonValue) -> Self {
        let hash = xxh64(resources.to_string().as_bytes(), 0);
        Self {
            resources,
            version_info: hash.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SemanticVersion {
    major_number: u8,
    minor_number: u8,
    patch: u8,
}

impl std::fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "{}.{}.{}",
            self.major_number, self.minor_number, self.patch
        ))
    }
}

#[derive(Serialize, Deserialize)]
struct BuildVersion {
    version: SemanticVersion,
}

#[derive(Serialize, Deserialize)]
struct Locality {
    region: Option<String>,
    zone: Option<String>,
    sub_zone: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Node {
    id: Option<String>,
    cluster: String,
    metadata: HashMap<String, JsonValue>,
    build_version: Option<String>,
    locality: Option<Locality>,
    user_agent_build_version: Option<BuildVersion>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscoveryRequest {
    node: Node,
    resource_names: Vec<String>,
    pub version_info: String,
}

impl DiscoveryRequest {
    pub fn new(cluster: String, version: String, resource_names: Vec<String>) -> Self {
        Self {
            node: Node {
                id: None,
                cluster,
                metadata: HashMap::new(),
                build_version: Some(version),
                locality: None,
                user_agent_build_version: None,
            },
            version_info: "0".to_string(),
            resource_names,
        }
    }
    pub fn envoy_version(&self) -> String {
        if let Some(v) = &self.node.build_version {
            v.to_string()
        } else if let Some(v) = &self.node.user_agent_build_version {
            v.version.to_string()
        } else {
            panic!("No envoy version")
        }
    }
    pub fn cluster(&self) -> &str {
        &self.node.cluster
    }
}
