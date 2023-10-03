use crate::sources::Source;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::path::PathBuf;

#[derive(Deserialize)]
struct XdsTemplate {
    path: PathBuf,
    scope: String,
    version: String,
    priority: Option<u8>,
}

#[derive(Deserialize)]
struct Settings {
    templates: Vec<XdsTemplate>,
    sources: Vec<Source>,
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
