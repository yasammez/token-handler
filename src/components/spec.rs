/// Config file types

use serde_derive::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Spec {
    #[serde(default = "_default_8080")]
    pub port: u16,
    #[serde(rename = "key", serialize_with = "hcl::ser::labeled_block")]
    pub keys: hcl::Map<String, KeySpec>,
    #[serde(default = "_default_30")]
    pub clock_skew: u16,
    #[serde(default)]
    pub expose_errors: bool,
    #[serde(rename = "bridge", serialize_with = "hcl::ser::labeled_block")]
    pub bridges: hcl::Map<String, BridgeSpec>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BridgeSpec {
    pub idp: String,
    pub client: String,
    #[serde(rename = "secret")]
    pub client_secret: String,
    #[serde(default = "_default_openid")]
    pub scope: String,
    #[serde(rename = "api", serialize_with = "hcl::ser::labeled_block")]
    pub apis: hcl::Map<String, ApiSpec>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ApiSpec {
    pub backend: String,
    #[serde(default = "_default_headers")]
    pub headers: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct KeySpec {
    pub value: String,
    #[serde(default)]
    pub active: bool,
}

const fn _default_8080() -> u16 { 8080 }
const fn _default_30() -> u16 { 30 }
fn _default_openid() -> String { "openid".into() }
fn _default_headers() -> Vec<String> { vec!["content-type".into() ]}
