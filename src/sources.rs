use pyo3::prelude::*;
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
pub enum Source {
    Inline { data: String },
    Python { code: String },
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
                    .call1(())
                    .expect("Main function failed")
                    .extract::<String>()
                    .expect("Could not parse main function return value as a string")
            })),
        }
    }
}
