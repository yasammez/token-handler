use serde_derive::{Serialize, Deserialize};
use crate::components::config::Bridge;

#[derive(Serialize, Deserialize, Debug)]
pub struct LoginCookie {
    pub state: String,
    pub nonce: String,
    pub bff_redirect_uri: String,
    pub post_login_redirect: String,
    pub code_verifier: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionCookie {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
}

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub refresh_expires_in: u32,
    pub refresh_token: String,
    pub token_type: String,
    #[serde(rename = "not-before-policy")]
    pub not_before_policy: u32,
    pub session_state: String,
    pub scope: String,
    pub id_token: String,
}

#[derive(Deserialize, Debug)]
pub struct IdTokenClaims {
    pub nonce: String,
    pub preferred_username: String,
}

#[derive(Deserialize, Debug)]
pub struct AccessTokenClaims {
    pub exp: u32,
    pub iat: u32,
    pub preferred_username: String,
}

#[derive(Deserialize, Debug)]
pub struct RefreshTokenClaims {
    pub exp: u32,
}

#[derive(Deserialize, Debug)]
pub struct IntrospectionClaims {
    pub active: bool,
}

#[derive(Deserialize, Debug)]
pub struct OpenidConfiguration {
    pub token_endpoint: String,
    pub authorization_endpoint: String,
    pub end_session_endpoint: String,
    pub introspection_endpoint: String,
}

#[derive(Serialize)]
pub struct ClientAuth<'a> {
    client_id: &'a str,
    client_secret: &'a str,
}

impl<'a> ClientAuth<'a> {
    pub fn new(bridge: &'a Bridge) -> Self {
        Self {
            client_id: &bridge.client,
            client_secret: &bridge.secret,
        }
    }
}

#[derive(Serialize)]
pub struct TokenRequest<'a> {
    #[serde(flatten)]
    pub auth: ClientAuth<'a>,
    #[serde(flatten)]
    pub details: TokenRequestDetails<'a>,
}

#[derive(Serialize)]
pub struct IntrospectionRequest<'a> {
    #[serde(flatten)]
    pub auth: ClientAuth<'a>,
    pub token: &'a str
}

#[derive(Serialize)]
#[serde(tag = "grant_type")]
pub enum TokenRequestDetails<'a> {
    #[serde(rename = "refresh_token")]
    RefreshToken { refresh_token: &'a str },
    #[serde(rename = "authorization_code")]
    AuthorizationCode { code: &'a str, redirect_uri: &'a str, code_verifier: &'a str },
}

#[derive(Serialize)]
pub struct LoginRequest<'a> {
        pub response_type: &'a str,
        pub scope: &'a str,
        pub client_id: &'a str,
        pub state: &'a str,
        pub redirect_uri: &'a str,
        pub code_challenge: &'a str,
        pub code_challenge_method: &'a str,
        pub nonce: &'a str,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub redirect: String,
}

#[derive(Deserialize)]
pub struct Login2Query {
    pub state: String,
    pub code: String,
}
