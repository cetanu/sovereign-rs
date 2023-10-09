use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;

use minijinja::Value as JinjaValue;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
#[cfg(feature = "s3")]
use rusoto_s3::S3;
use serde::{de, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_json::Value as YamlValue;

#[derive(Debug)]
pub enum Error {
    FileReadError(io::Error),
    HttpError(reqwest::Error),
    JsonParseError(serde_json::Error),
    YamlParseError(serde_yaml::Error),
    #[cfg(feature = "s3")]
    S3Error(rusoto_core::RusotoError<rusoto_s3::GetObjectError>),
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Format {
    Json,
    Yaml,
    Plaintext,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum Parsed {
    Text(String),
    Structured(JsonValue),
}

impl Into<JinjaValue> for Parsed {
    fn into(self) -> JinjaValue {
        match self {
            Self::Text(text) => JinjaValue::from_safe_string(text),
            Self::Structured(structured) => JinjaValue::from_serializable(&structured),
        }
    }
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct TemplateContext {
    deserialize_as: Format,
    data_source: DataSource,
}

impl TemplateContext {
    pub fn load(&self) -> Result<Parsed, Error> {
        let data: Vec<u8> = match &self.data_source {
            DataSource::File { path } => {
                let mut file = File::open(path).map_err(Error::FileReadError)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)
                    .map_err(Error::FileReadError)?;
                buffer
            }
            DataSource::Http { url, headers } => {
                let client = reqwest::blocking::Client::new();
                let mut response = client
                    .get(url)
                    .headers(headers.clone().unwrap_or_default())
                    .send()
                    .map_err(Error::HttpError)?;

                let mut buffer = Vec::new();
                response.copy_to(&mut buffer).unwrap();
                buffer
            }
            #[cfg(feature = "s3")]
            DataSource::S3 {
                bucket,
                key,
                region,
            } => {
                let s3_client = rusoto_s3::S3Client::new(
                    rusoto_core::Region::from_str(region.as_str())
                        .expect("Invalid region specified for S3 bucket"),
                );
                let get_req = rusoto_s3::GetObjectRequest {
                    bucket: bucket.clone(),
                    key: key.clone(),
                    ..Default::default()
                };
                let rt = tokio::runtime::Runtime::new().unwrap();
                let future = s3_client.get_object(get_req);
                let result = rt.block_on(future).map_err(Error::S3Error)?;
                let stream = result.body.unwrap();
                let mut buffer = Vec::new();
                stream
                    .into_blocking_read()
                    .read_to_end(&mut buffer)
                    .map_err(Error::FileReadError)?;
                buffer
            }
        };

        let parsed = match &self.deserialize_as {
            Format::Json => {
                let json: JsonValue =
                    serde_json::from_slice(&data).map_err(Error::JsonParseError)?;
                Parsed::Structured(json)
            }
            Format::Yaml => {
                let yaml: YamlValue =
                    serde_yaml::from_slice(&data).map_err(Error::YamlParseError)?;
                Parsed::Structured(yaml)
            }
            Format::Plaintext => {
                let text = String::from_utf8(data).unwrap();
                Parsed::Text(text)
            }
        };

        Ok(parsed)
    }
}

pub fn poll_context(ctx: &HashMap<String, TemplateContext>) -> JinjaValue {
    let map: HashMap<String, Parsed> = ctx
        .clone()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.load().unwrap()))
        .collect();
    JinjaValue::from(map)
}
