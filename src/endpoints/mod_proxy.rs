use actix_web::{HttpRequest, HttpResponse, Responder, web};
use actix_web::cookie::{Cookie, CookieJar, SameSite};
use actix_web::http::Method;
use futures_util::StreamExt;
use itertools::Itertools;
use log::info;
use reqwest::header;
use reqwest::header::{HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::error::{ApiError, Context};
use crate::components::config::{Api, Bridge};
use crate::systems::token::{claims, retrieve_token};
use crate::components::types::{AccessTokenClaims, RefreshTokenClaims, SessionCookie, TokenRequestDetails};
use crate::systems::cookies::{create, decode, SESSION_COOKIE_NAME};

pub async fn proxy(
    req: HttpRequest,
    method: Method,
    api: web::Data<Api>,
    path: web::Path<(String,)>,
    mut payload: web::Payload,
) -> Result<impl Responder, ApiError> {
    let request_path = path.into_inner().0;
    let bridge = api.bridge()?;
    let config = bridge.config()?;
    let cookie = req.cookie(SESSION_COOKIE_NAME).ok_or(ApiError::Unauthorized)?;
    let session = decode::<SessionCookie>(&cookie, &bridge)?;
    let mut jar = CookieJar::new();
    jar.add_original(cookie);
    let access_claims = claims::<AccessTokenClaims>(&session.access_token)?;
    let now = chrono::Utc::now().timestamp();
    let (access_token, refreshed) = if access_claims.exp as i64 - now < config.clock_skew as i64 {
        let refresh_claims = claims::<RefreshTokenClaims>(&session.refresh_token)?;
        if refresh_claims.exp as i64 - now < config.clock_skew as i64 {
            return Err(ApiError::NotLoggedIn).context("Refresh Token expired");
        }
        let renewed = get_new_token(&session.refresh_token, &bridge).await?;
        jar.add(renewed.0);
        (renewed.1, "|r")
    } else {
        (session.access_token, "  ")
    };

    let mut url = api.backend.clone();
    let path = format!("{}{}", url.path(), &request_path);
    url.set_path(&path);
    url.set_query(req.uri().query());

    let (tx, rx) = mpsc::channel(10);

    actix_web::rt::spawn(async move {
        while let Some(chunk) = payload.next().await {
            let _ = tx.send(chunk).await;
        }
    });

    let headers2: HeaderMap = api.headers.iter()
        .map(|name| {
            if name == header::COOKIE {
                let value = req.cookies().iter().flat_map(|x| x.iter())
                    // Don't expose our own cookie to the backend
                    .filter(|cookie| cookie.name() != SESSION_COOKIE_NAME)
                    .map(|cookie| cookie.to_string())
                    .join("; ");
                HeaderValue::from_str(&value).ok().map(|v| (name.clone(), v))
            } else {
                req.headers().get(name).map(|val| (name.clone(), val.clone()))
            }
        })
        .filter_map(|x| x)
        .collect::<HeaderMap>();

    let response  = bridge.config()?.reqwest
        .request(method, url)
        .headers(headers2)
        .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
        .body(reqwest::Body::wrap_stream(ReceiverStream::new(rx)))
        .send()
        .await?;

    let mut builder = HttpResponse::build(response.status());
    response.headers().iter().for_each(|(k, v)| { builder.append_header((k, v)); });
    jar.delta().for_each(|c| { builder.cookie(c.clone()); });
    info!("[{:<width$}::{}] proxy{} ({}) -- {:>7} {} : {}",
        bridge.id,
        api.id,
        refreshed,
        &access_claims.preferred_username,
        req.method().as_str(),
        request_path,
        response.status().as_u16(),
        width = bridge.config()?.log_padding - api.id.len() - 2,
    );
    Ok(builder.streaming(response.bytes_stream()))
}

async fn get_new_token(refresh_token: &str, bridge: &Bridge) -> Result<(Cookie<'static>, String), ApiError> {
    let response = retrieve_token(&bridge, TokenRequestDetails::RefreshToken { refresh_token }).await?;
    let access_token = response.access_token.clone();
    let cookie_value = SessionCookie {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        id_token: response.id_token,
    };
    let cookie = create(cookie_value, &bridge, SameSite::Strict)?;
    Ok((cookie, access_token))
}