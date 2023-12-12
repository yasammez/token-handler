use base64::{Engine as _, engine::{general_purpose}};
use reqwest::header;
use serde::Deserialize;
use crate::components::config::Bridge;
use crate::components::types::{ClientAuth, TokenRequest, TokenRequestDetails, TokenResponse};
use crate::error::{ApiError, Context};

/// Helper method for JWTs
pub fn claims<T: for<'a> Deserialize<'a>>(token: &str) -> Result<T, ApiError> {
    let base64 = token.as_bytes().split(|c| *c == 46).skip(1).next().ok_or(ApiError::BadGateway)?;
    let bytes = general_purpose::URL_SAFE_NO_PAD.decode(base64)?;
    Ok(serde_json::from_slice::<T>(&bytes)?)
}

/// Retrieve a set of tokens from the IDP
pub async fn retrieve_token<'a>(bridge: &Bridge, details: TokenRequestDetails<'a>) -> Result<TokenResponse, ApiError> {
    let response = bridge.config()?.reqwest.post(&bridge.get_idp_configuration().await?.token_endpoint)
        .header(header::CONTENT_TYPE, mime::APPLICATION_WWW_FORM_URLENCODED.as_ref())
        .body(serde_urlencoded::to_string(TokenRequest { auth: ClientAuth::new(&bridge), details })?)
        .send()
        .await?;
    let response = response.bytes().await?;
    Ok(serde_json::from_slice(response.as_ref())
        .context(String::from_utf8_lossy(response.as_ref()).to_string())?)
}
