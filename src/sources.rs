use pyo3::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum Source {
    Inline { data: JsonValue },
    PythonInline { code: String },
    PythonScript { path: PathBuf },
    Http { url: String },
    File { path: PathBuf },
}

/// Tag that indicates which cluster a bundle of instances is intended for
#[derive(Clone, Serialize)]
pub enum SourceDest {
    Any,
    Match(String),
}

/// A pre-coalesced group of instances, for one particular cluster
#[derive(Clone, Serialize)]
pub struct InstancesPackage {
    pub dest: SourceDest,
    pub instances: JsonValue,
}

fn call_python_code(code: &str) -> anyhow::Result<String> {
    Python::with_gil(|py| -> anyhow::Result<String> {
        let module = PyModule::from_code(py, code, "file.py", "module")?;
        Ok(module.getattr("main")?.call0()?.extract::<String>()?)
    })
}

fn read_file(path: &PathBuf) -> anyhow::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut content = String::new();
    reader.read_to_string(&mut content)?;
    Ok(content)
}

impl Source {
    pub fn get(&self) -> anyhow::Result<String> {
        match self {
            Source::Inline { data } => Ok(data.to_string()),
            Source::PythonInline { code } => call_python_code(code),
            Source::PythonScript { path } => call_python_code(&read_file(path)?),
            Source::Http { url } => {
                let u = url.clone();
                let future = async {
                    let client = Client::new();
                    client.get(u).send().await.unwrap().text().await.unwrap()
                };
                let handle = tokio::task::spawn(future);
                let result = tokio::task::block_in_place(|| {
                    let runtime = tokio::runtime::Handle::current();
                    runtime.block_on(handle)
                });
                Ok(result.unwrap())
            }
            Source::File { path } => read_file(path),
        }
    }
}

pub fn poll_sources(sources: &[Source]) -> anyhow::Result<Vec<InstancesPackage>> {
    let mut source_data = json! {[]};
    let borrow = source_data.as_array_mut().unwrap();
    for source in sources.iter() {
        let data = source.get()?;
        let json_value = serde_json::from_str::<JsonValue>(&data)?;
        let instances = json_value.as_array().unwrap();
        borrow.extend(instances.clone());
    }
    Ok(vec![InstancesPackage {
        dest: SourceDest::Any,
        instances: source_data,
    }])
}

pub fn poll_sources_into_buckets(
    sources: &[Source],
    source_match_key: &str,
) -> anyhow::Result<Vec<InstancesPackage>> {
    let mut ret = vec![];
    let mut buckets = HashMap::new();
    for source in sources.iter() {
        let data = source.get()?;
        let json_value = serde_json::from_str::<JsonValue>(&data)?;
        let instances = json_value.as_array().unwrap();

        for instance in instances {
            match instance.get(source_match_key) {
                // A list of string values is supported
                Some(JsonValue::Array(array)) => {
                    // Add a copy of the instance to every bucket
                    for bucket in array {
                        if let JsonValue::String(matched_key) = bucket {
                            buckets
                                .entry(matched_key.to_string())
                                .or_insert(json! {[]})
                                .as_array_mut()
                                .unwrap()
                                .push(instance.clone());
                        } else {
                            continue;
                        }
                    }
                }
                // or a singular string
                Some(JsonValue::String(key)) => {
                    buckets
                        .entry(key.to_string())
                        .or_insert(json! {[]})
                        .as_array_mut()
                        .unwrap()
                        .push(instance.clone());
                }
                _ => continue,
            }
        }
    }
    for (bucket, instances) in buckets.into_iter() {
        ret.push(InstancesPackage {
            dest: SourceDest::Match(bucket),
            instances,
        });
    }
    Ok(ret)
}
