use crate::context::TemplateContext;
use crate::sources::Source;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct XdsTemplate {
    path: PathBuf,
    envoy_version: String,
    resource_type: String,
}

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
}

#[derive(Deserialize)]
pub struct Settings {
    pub templates: Vec<XdsTemplate>,
    pub sources: Vec<Source>,
    pub template_context: HashMap<String, TemplateContext>,
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
