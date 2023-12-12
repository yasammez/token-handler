use std::borrow::Cow;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use actix_web::body::BoxBody;
use url::ParseError;
use serde_derive::Serialize;
use rmp_serde::encode::Error as EncError;
use rmp_serde::decode::Error as DecError;
use derive_more::{Display, Error, From};
use std::io::Error as IoError;
use std::str::Utf8Error;
use actix_web::http::header::{InvalidHeaderName, ToStrError};
use aes_gcm::aead;
use base64::DecodeError as B64Error;
use serde_json::{Error as JsonError, json};
use rand::Error as RandError;
use aead::Error as AeadError;
use log::debug;
use serde_urlencoded::ser::Error as UrlError;

#[derive(Display, Debug, Error, From)]
pub enum ConfigError {
    #[display(fmt = "malformed key '{}': {}", _0, _1)]
    MalformedKey(String, B64Error),
    #[display(fmt =  "key '{}' has invalid length {}: must be 32", _0, _1)]
    KeyLength(String, usize),
    #[display(fmt = "no active key")]
    NoActiveKey,
    #[display(fmt = "invalid Url '{}': {}", _0, _1)]
    InvalidUrl(String, ParseError),
    #[display(fmt = "invalid header name '{}'", _0)]
    InvalidHeader(InvalidHeaderName),
}

#[derive(Display, Debug, Error, From)]
pub enum ApiError {
    Aead(AeadError),
    B64(B64Error),
    BadGateway,
    Decode(DecError),
    Encode(EncError),
    Internal,
    Io(IoError),
    Json(JsonError),
    NotLoggedIn,
    Parse(ParseError),
    Rand(RandError),
    Reqwest(reqwest::Error),
    ToStr(ToStrError),
    Unauthorized,
    UnknownKey,
    UnknownRedirect,
    Url(UrlError),
    Utf8(Utf8Error),
    #[display(fmt = "{}", _1)]
    Context(Box<ApiError>, Cow<'static, str>),
}

impl ResponseError for ApiError {

    fn status_code(&self) -> StatusCode {
        match self {
            Self::Context(inner, _) => inner.status_code(),
            Self::Reqwest(_) | Self::Json(_) | Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::Unauthorized
                | Self::NotLoggedIn
                | Self::Decode(_)
                | Self::B64(_)
                | Self::Utf8(_)
                | Self::UnknownRedirect => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        debug!("Observed error: {:?}", self);
        let mut builder = HttpResponse::build(self.status_code());
        builder.extensions_mut().insert(ErrorResponse::from(self));
        builder.json(json!({}))
    }
}

pub trait Context<T> {
    fn context<C: Into<Cow<'static, str>>>(self, context: C) -> Result<T, ApiError>;
}

impl<T, E> Context<T> for Result<T, E> where E: Into<ApiError> {
    fn context<C: Into<Cow<'static, str>>>(self, context: C) -> Result<T, ApiError> {
        match self {
            Err(e) => Err(e.into().context(context)),
            Ok(x) => Ok(x),
        }
    }
}

impl ApiError {
    pub fn context<C: Into<Cow<'static, str>>>(self, context: C) -> Self {
        ApiError::Context(Box::new(self), context.into())
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    status: u16,
    error: Cow<'static, str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<Cow<'static, str>>,
}

impl From<&ApiError> for ErrorResponse {

    fn from(value: &ApiError) -> Self {
        match value {
            ApiError::Context(inner, context) => {
                let response = ErrorResponse::from(inner.as_ref());
                ErrorResponse { error: context.clone(), details: [&[response.error], &response.details[..]].concat(), ..response }
            },
            _ => ErrorResponse {
                status: value.status_code().as_u16(),
                error: format!("{:?}", value).into(),
                details: Vec::new(),
            }
        }
    }

}