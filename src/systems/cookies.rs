use serde::{Deserialize, Serialize};
use rmp_serde::encode::to_vec;
use rmp_serde::decode::from_slice;
use std::io::Write;
use std::str::from_utf8;
use actix_web::cookie::{Cookie, SameSite};
use base64::{Engine as _, engine::{general_purpose}};
use itertools::Itertools;
use rand::prelude::SliceRandom;
use crate::components::config::Bridge;
use crate::error::{ApiError, Context};
use crate::systems::crypto::{decrypt, encrypt};

pub const SESSION_COOKIE_NAME: &'static str = "bff-session";

/// Serialises, compresses, encrypts, and base64-encodes an instance and bakes it into a cookie
/// which is opaque for the client
pub fn create<'a, T: Serialize>(value: T, bridge: &Bridge, same_site: SameSite) -> Result<Cookie<'static>, ApiError> {
    let config = bridge.config()?;
    let key_id = config.active_keys.choose(&mut rand::thread_rng()).ok_or(ApiError::Internal)?;
    let key = &config.keys[key_id];

    let serialised = to_vec(&value)?;
    let compressed = compress(&serialised)?;
    let encrypted = encrypt(SESSION_COOKIE_NAME, &compressed, &key.value)?;
    Ok(Cookie::build(String::from(SESSION_COOKIE_NAME), [key_id.as_bytes(), &encrypted]
        .iter()
        .map(|x| general_purpose::URL_SAFE.encode(x))
        .join(".")
    )
        .http_only(true)
        .secure(true)
        .same_site(same_site)
        .path(format!("/bridge/{}", bridge.id))
        .finish())
}

/// base64-decodes, decrypts, decompresses and deserialises a cookie to an instance
pub fn decode<T: for<'a> Deserialize<'a>>(cookie: &Cookie, bridge: &Bridge) -> Result<T, ApiError> {
    let config = bridge.config()?;
    let (key_id, value) = cookie.value().split_once(".")
        .ok_or(ApiError::Unauthorized).context("malformed: expected '.'")?;
    let key_id = general_purpose::URL_SAFE.decode(&key_id)?;
    let decoded = general_purpose::URL_SAFE.decode(&value)?;
    let key_id = from_utf8(&key_id)?;
    let key = config.keys.get(key_id).ok_or(ApiError::UnknownKey)?;
    let decrypted = decrypt(cookie.name(), &decoded, &key.value)?;
    let decompressed = decompress(&decrypted)?;
    Ok(from_slice(&decompressed)?)
}

fn compress(input: &[u8]) -> Result<Vec<u8>, ApiError> {
    let mut writer = brotli::CompressorWriter::new(Vec::new(), 4096, 11, 22);
    writer.write_all(input)?;
    Ok(writer.into_inner())
}

fn decompress(input: &[u8]) -> Result<Vec<u8>, ApiError> {
    let mut decompressor = brotli::DecompressorWriter::new(Vec::new(), 4096);
    decompressor.write_all(input)?;
    decompressor.into_inner().map_err(|_| ApiError::Internal)
}