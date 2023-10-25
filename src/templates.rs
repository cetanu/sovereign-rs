use crate::context::DeserializeAs;
use minijinja::Value as JinjaValue;
use pyo3::prelude::*;
use serde::Deserialize;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

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

    pub fn call(&self, kwargs: JinjaValue) -> anyhow::Result<String> {
        Python::with_gil(|py| -> anyhow::Result<String> {
            let module = PyModule::from_code(
                py,
                &format!("{}\n{}", PY_BOILETPLATE, &self.source().unwrap()),
                &self.path.to_string_lossy(),
                "template",
            )?;
            Ok(module
                .getattr("main")?
                .call1((serde_json::to_string(&kwargs).unwrap(),))?
                .extract::<String>()?)
        })
    }
}
