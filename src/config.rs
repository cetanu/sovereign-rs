use crate::context::DeserializeAs;
use crate::context::TemplateContext;
use crate::sources::Source;
use config::{Config, ConfigError, Environment, File};
use minijinja::Value as JinjaValue;
use pyo3::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use tokio::time::Duration;

#[derive(Deserialize, Clone)]
pub struct XdsTemplate {
    path: PathBuf,
    envoy_version: String,
    resource_type: String,
    #[serde(default)]
    pub deserialize_as: DeserializeAs,
    pub call_python: Option<bool>,
}

const PY_BOILETPLATE: &str = r#"
import json

def main(kw):
    kwargs = json.loads(kw)
    result = [r for r in call(**kwargs)]
    return json.dumps(result)
"#;

impl XdsTemplate {
    pub fn name(&self) -> String {
        format!("{}/{}", self.envoy_version, self.resource_type)
    }

    pub fn source(&self) -> std::io::Result<String> {
        let file = std::fs::File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        Ok(content)
    }

    pub fn call(&self, kwargs: JinjaValue) -> String {
        Python::with_gil(|py| {
            let module = PyModule::from_code(
                py,
                &format!("{}\n{}", PY_BOILETPLATE, &self.source().unwrap()),
                &self.path.to_string_lossy(),
                "template",
            )
            .expect("Could not parse python code");
            module
                .getattr("main")
                .expect("No 'call' function in python template")
                .call1((serde_json::to_string(&kwargs).unwrap(),))
                .expect("Template function failed")
                .extract::<String>()
                .expect("Could not parse call function return value as a string")
        })
    }
}

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
