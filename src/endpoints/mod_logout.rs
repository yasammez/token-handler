use actix_web::{get, HttpRequest, HttpResponse, Responder, web};
use actix_web::cookie::{Cookie, SameSite};
use actix_web::cookie::time::OffsetDateTime;
use actix_web::http::header;
use log::info;
use crate::components::config::Bridge;
use crate::error::ApiError;
use crate::systems::token::claims;
use crate::components::types::{IdTokenClaims, SessionCookie};
use crate::systems::cookies::{decode, SESSION_COOKIE_NAME};
use serde_derive::Deserialize;

#[get("/logout")]
pub async fn logout(req: HttpRequest, bridge: web::Data<Bridge>, query: web::Query<LogoutQuery>) -> Result<impl Responder, ApiError> {
    let cookie = decode::<SessionCookie>(&req.cookie(SESSION_COOKIE_NAME).ok_or(ApiError::Unauthorized)?, &bridge)?;
    let logout_uri = &bridge.get_idp_configuration().await?.end_session_endpoint;
    let redirect = query.into_inner()
        .post_logout_redirect_uri
        .or_else(|| req.headers().get(header::REFERER).and_then(|h| h.to_str().ok()).map(|h| h.to_owned()))
        .ok_or(ApiError::UnknownRedirect)?;
    let id_token = &cookie.id_token;
    let location = format!("{logout_uri}?post_logout_redirect_uri={redirect}&id_token_hint={id_token}");
    let claims = claims::<IdTokenClaims>(&cookie.id_token)?;
    info!("[{:<width$}] logout  ({})", bridge.id, claims.preferred_username, width = bridge.config()?.log_padding);
    Ok(HttpResponse::TemporaryRedirect()
        .insert_header((header::LOCATION, location))
        .cookie(Cookie::build(SESSION_COOKIE_NAME, "")
            .same_site(SameSite::Lax)
            .secure(true)
            .expires(OffsetDateTime::UNIX_EPOCH)
            .path(format!("/bridge/{}", bridge.id))
            .finish())
        .finish())
}

#[derive(Deserialize)]
pub struct LogoutQuery {
    post_logout_redirect_uri: Option<String>,
}