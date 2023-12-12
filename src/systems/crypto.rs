use aes_gcm::{AeadInPlace, Aes256Gcm, KeyInit};
use aes_gcm::aead::{Aead, Payload};
use base64::Engine;
use base64::engine::general_purpose;
use rand::RngCore;
use sha2::digest::generic_array::GenericArray;
use sha2::{Digest, Sha256};
use crate::error::ApiError;

pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const TAG_LEN: usize = 16;

pub fn encrypt(name: &str, value: &[u8], key: &[u8]) -> Result<Vec<u8>, ApiError> {
    let mut data = vec![0; NONCE_LEN + value.len() + TAG_LEN];
    let (nonce, in_out) = data.split_at_mut(NONCE_LEN);
    let (in_out, tag) = in_out.split_at_mut(value.len());
    in_out.copy_from_slice(value);
    rand::thread_rng().try_fill_bytes(nonce)?;
    let nonce = GenericArray::clone_from_slice(nonce);
    let aad_tag = Aes256Gcm::new(GenericArray::from_slice(key))
        .encrypt_in_place_detached(&nonce, name.as_bytes(), in_out)?;
    tag.copy_from_slice(&aad_tag);

    Ok(data)
}

pub fn decrypt(name: &str, value: &[u8], key: &[u8]) -> Result<Vec<u8>, ApiError> {
    let data = value;
    if data.len() <= NONCE_LEN {
        return Err(ApiError::Unauthorized);
    }
    let (nonce, cipher) = data.split_at(NONCE_LEN);
    let payload = Payload { msg: cipher, aad: name.as_bytes() };
    Aes256Gcm::new(GenericArray::from_slice(key))
        .decrypt(GenericArray::from_slice(nonce), payload)
        .map_err(|_| ApiError::Unauthorized)
}

pub fn hash(input: &str) -> Result<String, ApiError> {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    Ok(general_purpose::URL_SAFE_NO_PAD.encode(&result))
}