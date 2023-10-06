use pyo3::prelude::*;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::error::Error;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
pub enum Source {
    Inline { data: JsonValue },
    Python { code: String },
    Http { url: Url },
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
            Source::Http { url } => todo!(),
            Source::File { path } => todo!(),
        }
    }
}
