use actix_web::http::header;
use actix_web::{get, HttpRequest, HttpResponse, Responder, web};
use actix_web::cookie::SameSite;
use actix_web::dev::ConnectionInfo;
use nanoid::nanoid;
use crate::systems::cookies::{decode, create, SESSION_COOKIE_NAME};
use crate::components::types::{IdTokenClaims, Login2Query, LoginCookie, LoginQuery, LoginRequest, SessionCookie, TokenRequestDetails};
use log::info;
use crate::components::config::Bridge;
use crate::error::ApiError;
use crate::systems::crypto::hash;
use crate::systems::token::{claims, retrieve_token};

#[get("/login")]
pub async fn login(bridge: web::Data<Bridge>, query: web::Query<LoginQuery>, conn: ConnectionInfo) -> Result<impl Responder, ApiError> {
    let state = nanoid!(10);
    let nonce = nanoid!(10);
    let code_verifier = nanoid!(43);
    let bff_redirect_uri = format!("{}://{}/bridge/{}/login2", conn.scheme(), conn.host(), &bridge.id);
    let url = &bridge.get_idp_configuration().await?.authorization_endpoint;

    let login_query = serde_urlencoded::to_string(LoginRequest {
        response_type: "code",
        code_challenge_method: "S256",
        scope: &bridge.scope,
        client_id: &bridge.client,
        nonce: &nonce,
        state: &state,
        redirect_uri: &bff_redirect_uri,
        code_challenge: &hash(&code_verifier)?,
    })?;

    let cookie_value = LoginCookie {
        nonce,
        state,
        code_verifier,
        bff_redirect_uri,
        post_login_redirect: query.into_inner().redirect,
    };

    Ok(HttpResponse::TemporaryRedirect()
        .insert_header((header::LOCATION, format!("{url}?{login_query}")))
        .cookie(create(cookie_value, &bridge, SameSite::Lax)?)
        .finish())
}

#[get("/login2")]
pub async fn login2(
    req: HttpRequest,
    query: web::Query<Login2Query>,
    bridge: web::Data<Bridge>,
) -> Result<impl Responder, ApiError> {
    let cookie = decode::<LoginCookie>(&req.cookie(&SESSION_COOKIE_NAME).ok_or(ApiError::Unauthorized)?, &bridge)?;

    // verify state
    if cookie.state != query.state {
        return Err(ApiError::Unauthorized);
    }

    let response = retrieve_token(&bridge, TokenRequestDetails::AuthorizationCode {
        code: &query.code,
        redirect_uri: &cookie.bff_redirect_uri,
        code_verifier: &cookie.code_verifier,
    }).await?;

    // verify nonce
    let claims = claims::<IdTokenClaims>(&response.id_token)?;
    if claims.nonce != cookie.nonce {
        return Err(ApiError::Unauthorized);
    }

    info!("[{:<width$}] login   ({})", bridge.id, claims.preferred_username, width = bridge.config()?.log_padding);

    let cookie_value = SessionCookie {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        id_token: response.id_token,
    };
    Ok(HttpResponse::TemporaryRedirect()
        .insert_header((header::LOCATION, cookie.post_login_redirect))
        .cookie(create(cookie_value, &bridge, SameSite::Strict)?)
        .finish())
}