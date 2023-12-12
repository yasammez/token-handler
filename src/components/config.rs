/// Server config types

use base64::Engine;
use base64::engine::general_purpose;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock, Weak};
use log::info;
use reqwest::Client;
use reqwest::header::{HeaderName, InvalidHeaderName};
use serde::ser::SerializeSeq;
use serde::Serializer;
use serde_derive::Serialize;
use url::Url;
use crate::components::spec::{ApiSpec, BridgeSpec, Spec};
use crate::components::types::OpenidConfiguration;
use crate::error::{ApiError, ConfigError, Context};

#[derive(Serialize)]
pub struct Config {
    pub port: u16,
    pub clock_skew: u16,
    pub expose_errors: bool,
    #[serde(rename = "key", serialize_with = "hcl::ser::labeled_block")]
    pub keys: HashMap<String, Key>,
    #[serde(skip_serializing)]
    pub active_keys: Vec<String>,
    #[serde(skip_serializing)]
    pub log_padding: usize,
    #[serde(skip_serializing)]
    pub reqwest: Client,
    #[serde(rename = "bridge", serialize_with = "hcl::ser::labeled_block")]
    pub bridges: HashMap<String, Arc<Bridge>>,
}

impl TryFrom<&Spec> for Arc<Config> {
    type Error = ConfigError;

    fn try_from(value: &Spec) -> Result<Self, ConfigError> {
        let (keys, active_keys) = value.keys.iter()
            .try_fold((HashMap::with_capacity(value.keys.len()), Vec::new()), |mut acc: (HashMap<String, Key>, Vec<String>), (k, v)| {
                let value = general_purpose::STANDARD.decode(&v.value)
                    .map_err(|e| ConfigError::MalformedKey(k.clone(), e))
                    .and_then(|x| match x.len() {
                        32 => Ok(Key { value: x, active: v.active }),
                        l => Err(ConfigError::KeyLength(k.clone(), l)),
                    })?;
                acc.0.insert(k.clone(), value);
                if v.active { acc.1.push(k.clone()) };
                Ok::<(HashMap<String, Key>, Vec<String>), ConfigError>(acc)
            })?;
        let log_padding = value.bridges.iter().fold(0, |acc: usize, (k, v)|
            usize::max(acc, 2 + k.len() + v.apis.iter().fold(0, |acc: usize, (k, _)|
                usize::max(acc, k.len()))));
        if active_keys.is_empty() {
            return Err(ConfigError::NoActiveKey);
        }
        let bridges = value.bridges.iter()
            .map(|(id, bridge)| BridgeBuilder::new(id, bridge))
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok(Arc::new_cyclic(|me| {
            Config {
                keys,
                active_keys,
                clock_skew: value.clock_skew,
                port: value.port,
                log_padding,
                expose_errors: value.expose_errors,
                reqwest: Client::default(),
                bridges: bridges.into_iter().map(|bridge| (bridge.id.clone(), bridge.connect(me.clone()))).collect(),
            }
        }))
    }
}

/// Configures a client binding to an IDP
#[derive(Serialize)]
pub struct Bridge {
    #[serde(skip_serializing)]
    config: Weak<Config>,
    #[serde(skip_serializing)]
    pub id: String,
    pub idp: String,
    pub client: String,
    #[serde(serialize_with = "serialize_asterisks")]
    pub secret: String,
    pub scope: String,
    #[serde(rename = "api", serialize_with = "hcl::ser::labeled_block")]
    pub apis: HashMap<String, Arc<Api>>,
    #[serde(skip_serializing)]
    idp_configuration: RwLock<Option<Arc<OpenidConfiguration>>>,
}

pub struct BridgeBuilder {
    pub id: String,
    pub idp_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub apis: Vec<ApiBuilder>,
    idp_configuration: RwLock<Option<Arc<OpenidConfiguration>>>,
}

impl BridgeBuilder {
    pub fn new(id: &str, value: &BridgeSpec) -> Result<BridgeBuilder, ConfigError> {
        let apis = value.apis.iter()
            .map(|(id, api)| { ApiBuilder::new(id, api) })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BridgeBuilder {
            id: id.into(),
            idp_url: value.idp.clone(),
            idp_configuration: RwLock::new(None),
            client_id: value.client.clone(),
            client_secret: value.client_secret.clone(),
            scope: value.scope.clone(),
            apis,
        })
    }

    pub fn connect(self, config: Weak<Config>) -> Arc<Bridge> {
        Arc::new_cyclic(|me| Bridge {
            config,
            id: self.id,
            idp: self.idp_url,
            idp_configuration: self.idp_configuration,
            client: self.client_id,
            secret: self.client_secret,
            scope: self.scope,
            apis: self.apis.into_iter().map(|api| (api.id.clone(), api.connect(me.clone()))).collect(),
        })
    }

}

impl Bridge {
    pub fn config(&self) -> Result<Arc<Config>, ApiError> {
        Ok(self.config.upgrade().ok_or(ApiError::Internal).context("finding config from bridge")?)
    }

    pub async fn get_idp_configuration(&self) -> Result<Arc<OpenidConfiguration>, ApiError> {
        let c = self.idp_configuration.read().unwrap();
        match *c {
            Some(ref config) => Ok(config.clone()),
            None => {
                let config = self.config()?.reqwest.get(format!("{}/.well-known/openid-configuration", self.idp))
                    .send().await?.json::<OpenidConfiguration>().await?;
                info!("[{:<width$}] loaded IDP configuration", self.id, width = self.config()?.log_padding);
                drop(c);
                let mut lock = self.idp_configuration.write().unwrap();
                let config = lock.insert(Arc::new(config));
                Ok(config.clone())
            }
        }
    }
}

#[derive(Serialize)]
pub struct Api {
    #[serde(skip_serializing)]
    pub bridge: Weak<Bridge>,
    #[serde(skip_serializing)]
    pub id: String,
    pub backend: Url,
    #[serde(serialize_with = "serialize_header_names")]
    pub headers: Vec<HeaderName>,
}

fn serialize_header_names<S>(input: &Vec<HeaderName>, ser: S) -> Result<S::Ok, S::Error> where S: Serializer {
    let mut seq = ser.serialize_seq(Some(input.len()))?;
    for i in input.iter() {
        seq.serialize_element(i.as_str())?;
    }
    seq.end()
}

pub struct ApiBuilder {
    pub id: String,
    pub backend: Url,
    pub headers: Vec<HeaderName>,
}

impl ApiBuilder {
    pub fn new(id: &str, value: &ApiSpec) -> Result<Self, ConfigError> {
        let mut backend = value.backend.clone();
        if !backend.ends_with("/") {
            backend.push('/');
        }
        Ok(ApiBuilder {
            id: id.into(),
            backend: Url::from_str(&backend)
                .map_err(|p| ConfigError::InvalidUrl(backend, p))?,
            headers: value.headers.iter()
                .map(|x| x.to_lowercase())
                .map(|x| HeaderName::from_lowercase(x.as_bytes()))
                .collect::<Result<Vec<_>, InvalidHeaderName>>()?,
        })
    }

    pub fn connect(self, bridge: Weak<Bridge>) -> Arc<Api> {
        Arc::new(Api { bridge, id: self.id, backend: self.backend, headers: self.headers })
    }

}

impl Api {
    pub fn bridge(&self) -> Result<Arc<Bridge>, ApiError> {
        Ok(self.bridge.upgrade().ok_or(ApiError::Internal).context("finding bridge from API")?)
    }
}

#[derive(Serialize)]
pub struct Key {
    #[serde(serialize_with = "serialize_asterisks")]
    pub value: Vec<u8>,
    pub active: bool,
}

fn serialize_asterisks<S, T>(_: T, ser: S) -> Result<S::Ok, S::Error> where S: Serializer {
    ser.serialize_str("*****")
}

