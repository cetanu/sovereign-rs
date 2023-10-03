use pyo3::prelude::*;
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Deserialize)]
#[serde(tag = "Type", content = "config", rename_all = "lowercase")]
pub enum Source {
    Inline { data: String },
    Python { code: String },
}

impl Source {
    fn get(self) -> Result<String, Box<dyn Error>> {
        match self {
            Source::Inline { data } => Ok(data),
            Source::Python { code } => Ok(Python::with_gil(|py| {
                let result: PyResult<String> =
                    py.eval(&code, None, None).map(|val| val.extract().unwrap());

                match result {
                    Ok(output) => Ok(output),
                    Err(err) => Err(Box::new(err)),
                }
            })?),
        }
    }
}
