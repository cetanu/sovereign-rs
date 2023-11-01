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
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    cluster: String,
    metadata: HashMap<String, JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locality: Option<Locality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_agent_build_version: Option<BuildVersion>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscoveryRequest {
    node: Node,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_info: Option<String>,
}

impl DiscoveryRequest {
    pub fn new(cluster: String, version: String, resource_names: Option<Vec<String>>) -> Self {
        Self {
            node: Node {
                id: None,
                cluster,
                metadata: HashMap::new(),
                build_version: Some(version),
                locality: None,
                user_agent_build_version: None,
            },
            version_info: Some("0".to_string()),
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
    pub fn resource_names(&self) -> Vec<String> {
        self.resource_names.to_owned().unwrap_or_default()
    }
}
