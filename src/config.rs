use crate::context::TemplateContext;
use crate::sources::Source;
use crate::templates::XdsTemplate;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use tokio::time::Duration;

#[derive(Deserialize, Clone)]
pub struct NodeMatching {
    pub source_key: String,
}

#[derive(Deserialize, Clone)]
pub struct TemplateContextConfig {
    pub items: HashMap<String, TemplateContext>,
    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_duration"
    )]
    pub interval: Duration,
}

#[derive(Deserialize, Clone)]
pub struct SourceConfig {
    pub items: Vec<Source>,
    #[serde(
        deserialize_with = "deserialize_duration",
        default = "default_duration"
    )]
    pub interval: Duration,
}

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub templates: Vec<XdsTemplate>,
    pub sources: Option<SourceConfig>,
    pub template_context: Option<TemplateContextConfig>,
    pub node_matching: Option<NodeMatching>,
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

fn default_duration() -> Duration {
    Duration::from_secs(30)
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let config_path =
            env::var("SOVEREIGN_CONFIG_PATH").unwrap_or_else(|_| "sovereign.yaml".into());

        let mut s = Config::builder();
        for path in config_path.split(',') {
            s = s.add_source(File::with_name(path));
        }
        s = s.add_source(Environment::with_prefix("SOVEREIGN"));
        s.build()?.try_deserialize()
    }
}
