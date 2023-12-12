use actix_web::{HttpRequest, HttpResponse, Responder, web, get};
use actix_web::http::header;
use base64::Engine;
use base64::engine::general_purpose;
use log::info;
use crate::components::config::Bridge;
use crate::error::ApiError;
use crate::components::types::{AccessTokenClaims, ClientAuth, IntrospectionClaims, IntrospectionRequest, SessionCookie};
use crate::systems::cookies::{decode, SESSION_COOKIE_NAME};
use crate::error::Context;
use crate::systems::token::claims;

#[get("/me")]
pub async fn me(req: HttpRequest, bridge: web::Data<Bridge>) -> Result<impl Responder, ApiError> {
    let cookie = req.cookie(SESSION_COOKIE_NAME)
        .ok_or(ApiError::NotLoggedIn)
        .context("No session cookie")?;
    let cookie = decode::<SessionCookie>(&cookie, &bridge)
        .map_err(|_| ApiError::NotLoggedIn)
        .context("Couldn't decode session cookie")?;
    let access_claims = claims::<AccessTokenClaims>(&cookie.access_token)?;

    // perform token introspection
    let response = bridge.config()?.reqwest.post(&bridge.get_idp_configuration().await?.introspection_endpoint)
        .header(header::CONTENT_TYPE, mime::APPLICATION_WWW_FORM_URLENCODED.as_ref())
        .body(serde_urlencoded::to_string(IntrospectionRequest { auth: ClientAuth::new(&bridge), token: &cookie.id_token })?)
        .send()
        .await.context("posting token introspection to IDP")?
        .json::<IntrospectionClaims>()
        .await.context("deserializing introspection claims")?;

    if !response.active {
        return Err(ApiError::Unauthorized).context("session inactive");
    }

    let base64 = cookie.id_token.as_bytes().split(|c| *c == 46).skip(1).next().ok_or(ApiError::BadGateway)?;
    let bytes = general_purpose::URL_SAFE_NO_PAD.decode(base64)?;

    info!("[{:<width$}] me      ({})", bridge.id, &access_claims.preferred_username, width = bridge.config()?.log_padding);
    Ok(HttpResponse::Ok().insert_header((header::CONTENT_TYPE, mime::APPLICATION_JSON)).body(bytes))
}