use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;

use minijinja::Value as JinjaValue;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
#[cfg(feature = "s3")]
use rusoto_s3::S3;
use serde::{de, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_json::Value as YamlValue;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DeserializeAs {
    Json,
    Yaml,
    Plaintext,
}

impl Default for DeserializeAs {
    fn default() -> Self {
        Self::Json
    }
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Parsed {
    Text(String),
    Structured(JsonValue),
}

impl From<Parsed> for JinjaValue {
    fn from(val: Parsed) -> Self {
        match val {
            Parsed::Text(text) => JinjaValue::from_safe_string(text),
            Parsed::Structured(structured) => JinjaValue::from_serializable(&structured),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
enum DataSource {
    File {
        path: String,
    },
    Http {
        url: String,
        #[serde(deserialize_with = "deserialize_headermap")]
        headers: Option<HeaderMap>,
    },
    #[cfg(feature = "s3")]
    S3 {
        bucket: String,
        key: String,
        region: String,
    },
}

fn deserialize_headermap<'de, D>(deserializer: D) -> Result<Option<HeaderMap>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let val: JsonValue = Deserialize::deserialize(deserializer)?;
    let mut ret = HeaderMap::new();
    if let Some(map) = val.as_object() {
        for (k, v) in map {
            let name = HeaderName::from_str(k).map_err(serde::de::Error::custom)?;
            let value_str = v
                .as_str()
                .ok_or_else(|| serde::de::Error::custom("Expected a string"))?;
            let value = HeaderValue::from_str(value_str).map_err(serde::de::Error::custom)?;
            ret.insert(name, value);
        }
        Ok(Some(ret))
    } else {
        Ok(None)
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub struct TemplateContext {
    #[serde(default)]
    deserialize_as: DeserializeAs,
    data_source: DataSource,
}

impl TemplateContext {
    pub fn load(&self) -> anyhow::Result<Parsed> {
        let data: Vec<u8> = match &self.data_source {
            DataSource::File { path } => {
                let mut file = File::open(path)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                buffer
            }
            DataSource::Http { url, headers } => {
                let u = url.clone();
                let h = headers.clone();
                let future = async {
                    let client = reqwest::Client::new();
                    client
                        .get(u)
                        .headers(h.unwrap_or_default())
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap()
                };
                let handle = tokio::task::spawn(future);
                let result = tokio::task::block_in_place(|| {
                    let runtime = tokio::runtime::Handle::current();
                    runtime.block_on(handle)
                });
                result.unwrap().as_bytes().to_vec()
            }
            #[cfg(feature = "s3")]
            DataSource::S3 {
                bucket,
                key,
                region,
            } => {
                let s3_client =
                    rusoto_s3::S3Client::new(rusoto_core::Region::from_str(region.as_str())?);
                let get_req = rusoto_s3::GetObjectRequest {
                    bucket: bucket.clone(),
                    key: key.clone(),
                    ..Default::default()
                };
                let rt = tokio::runtime::Runtime::new()?;
                let future = s3_client.get_object(get_req);
                let result = rt.block_on(future)?;
                let stream = result.body.unwrap();
                let mut buffer = Vec::new();
                stream.into_blocking_read().read_to_end(&mut buffer)?;
                buffer
            }
        };

        let parsed = match &self.deserialize_as {
            DeserializeAs::Json => {
                let json: JsonValue = serde_json::from_slice(&data)?;
                Parsed::Structured(json)
            }
            DeserializeAs::Yaml => {
                let yaml: YamlValue = serde_yaml::from_slice(&data)?;
                Parsed::Structured(yaml)
            }
            DeserializeAs::Plaintext => {
                let text = String::from_utf8(data)?;
                Parsed::Text(text)
            }
        };

        Ok(parsed)
    }
}

pub fn poll_context(ctx: &HashMap<String, TemplateContext>) -> JinjaValue {
    let map: HashMap<String, Parsed> = ctx
        .iter()
        .map(|(k, v)| (k.to_string(), v.load().unwrap()))
        .collect();
    JinjaValue::from(map)
}
