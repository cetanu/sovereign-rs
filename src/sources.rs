use pyo3::prelude::*;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::error::Error;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
pub enum Source {
    Inline { data: JsonValue },
    Python { code: String },
    Http { url: String },
    File { path: PathBuf },
}

impl Source {
    pub fn get(&self) -> Result<String, Box<dyn Error>> {
        match self {
            Source::Inline { data } => Ok(data.to_string()),
            Source::Python { code } => Ok(Python::with_gil(|py| {
                let module = PyModule::from_code(py, &code, "file.py", "module")
                    .expect("Could not parse python code");
                module
                    .getattr("main")
                    .expect("No main function in python code")
                    .call0()
                    .expect("Main function failed")
                    .extract::<String>()
                    .expect("Could not parse main function return value as a string")
            })),
            Source::Http { url } => {
                let client = Client::new();
                Ok(client.get(url).send()?.text()?)
            }
            Source::File { path } => {
                let file = std::fs::File::open(&path)?;
                let mut reader = BufReader::new(file);
                let mut content = String::new();
                reader.read_to_string(&mut content)?;
                Ok(content)
            }
        }
    }
}

pub fn poll_sources(sources: &[Source]) -> JsonValue {
    let mut ret = json! {[]};
    for source in sources.iter() {
        let val = source.get();
        if let Ok(s) = val {
            if let Ok(json_value) = serde_json::from_str::<JsonValue>(&s) {
                if let Some(s) = ret.as_array_mut() {
                    if let Some(instances) = json_value.as_array() {
                        s.extend(instances.clone());
                    }
                }
            }
        } else {
            panic!("Failed to get from source");
        }
    }
    ret
}
