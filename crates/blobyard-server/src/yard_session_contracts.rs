use crate::error::ApiError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_contract::YARD_CONTINUATION_LIFETIME_MS;
use blobyard_core::{SecretString, WebYardOrigin};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CONTINUATION_PREFIX: &str = "byc_";
const DOMAIN: &[u8] = b"yard-continuation-v1";
const VERSION: u8 = 1;
const MAXIMUM_RETURN_PATH_BYTES: usize = 2_048;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContinuationClaims {
    e: u64,
    h: String,
    n: String,
    p: String,
    v: u8,
}

impl ContinuationClaims {
    pub(crate) fn host_label(&self) -> &str {
        &self.h
    }

    pub(crate) fn return_path(&self) -> &str {
        &self.p
    }
}

pub(crate) fn derive_key(capability_key: &SecretString) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(capability_key.expose_secret().as_bytes());
    digest.update([0]);
    digest.update(DOMAIN);
    digest.finalize().into()
}

pub(crate) fn issue(
    key: &[u8; 32],
    host_label: &str,
    return_path: &str,
    now_ms: u64,
) -> Result<SecretString, ApiError> {
    if !valid_host_label(host_label) {
        return Err(ApiError::internal());
    }
    let claims = ContinuationClaims {
        e: now_ms
            .checked_add(YARD_CONTINUATION_LIFETIME_MS)
            .ok_or_else(ApiError::internal)?,
        h: host_label.to_owned(),
        n: uuid::Uuid::new_v4().simple().to_string(),
        p: normalize_return_path(return_path).to_owned(),
        v: VERSION,
    };
    encoded_continuation(serde_json::to_vec(&claims), HmacSha256::new_from_slice(key))
}

fn encoded_continuation(
    payload: serde_json::Result<Vec<u8>>,
    signer: Result<HmacSha256, hmac::digest::InvalidLength>,
) -> Result<SecretString, ApiError> {
    let payload = issue_payload(payload)?;
    let signature = signature_from(signer, &payload)?;
    issued_secret(SecretString::new(format!(
        "{CONTINUATION_PREFIX}{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        hex::encode(signature)
    )))
}

pub(crate) fn verify(
    key: &[u8; 32],
    continuation: &SecretString,
    now_ms: u64,
) -> Result<ContinuationClaims, ()> {
    let raw = continuation
        .expose_secret()
        .strip_prefix(CONTINUATION_PREFIX)
        .ok_or(())?;
    let (encoded, signature_hex) = raw.split_once('.').ok_or(())?;
    if encoded.is_empty()
        || signature_hex.len() != 64
        || !signature_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(());
    }
    let payload = URL_SAFE_NO_PAD.decode(encoded).map_err(|_error| ())?;
    verified_payload(
        &payload,
        hex::decode(signature_hex),
        HmacSha256::new_from_slice(key),
    )?;

    let claims: ContinuationClaims = serde_json::from_slice(&payload).map_err(|_error| ())?;
    let canonical = serde_json::to_vec(&claims);
    validate_claims(claims, &payload, now_ms, canonical)
}

fn verified_payload(
    payload: &[u8],
    supplied_signature: Result<Vec<u8>, hex::FromHexError>,
    verifier: Result<HmacSha256, hmac::digest::InvalidLength>,
) -> Result<(), ()> {
    let supplied_signature = decoded_signature(supplied_signature)?;
    let mut verifier = verifier_result(verifier)?;
    verifier.update(payload);
    verifier
        .verify_slice(&supplied_signature)
        .map_err(|_error| ())
}

fn validate_claims(
    mut claims: ContinuationClaims,
    payload: &[u8],
    now_ms: u64,
    canonical: serde_json::Result<Vec<u8>>,
) -> Result<ContinuationClaims, ()> {
    if canonical_payload(canonical)? != payload
        || claims.v != VERSION
        || claims.e <= now_ms
        || !valid_host_label(&claims.h)
        || !is_lower_hex(&claims.n, 32)
    {
        return Err(());
    }
    claims.p = normalize_return_path(&claims.p).to_owned();
    Ok(claims)
}

pub(crate) fn normalize_return_path(value: &str) -> &str {
    if value.starts_with('/')
        && !value.starts_with("//")
        && !value.starts_with("/\\")
        && value.len() <= MAXIMUM_RETURN_PATH_BYTES
        && !value.chars().any(char::is_control)
        && value != "/.blobyard"
        && !value.starts_with("/.blobyard/")
    {
        value
    } else {
        "/"
    }
}

pub(crate) fn has_token_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| is_lower_hex(suffix, 64))
}

pub(crate) fn identity_authority(origin: &str) -> Option<String> {
    let parsed = url::Url::parse(origin).ok()?;
    parsed.host_str()?;
    Some(parsed[url::Position::BeforeHost..url::Position::AfterPort].to_owned())
}

pub(crate) fn yard_host_label(origin: &str, authority: &str) -> Option<String> {
    let origin = WebYardOrigin::new(origin).ok()?;
    let suffix = format!(".{}", origin.authority());
    let label = authority.strip_suffix(&suffix)?;
    valid_host_label(label).then(|| label.to_owned())
}

pub(crate) fn yard_url(origin: &str, host_label: &str) -> Result<String, ApiError> {
    WebYardOrigin::new(origin)
        .map_err(|_error| ApiError::internal())?
        .url_for(host_label)
        .map_err(|_error| ApiError::internal())
}

pub(crate) fn login_url(origin: &str, continuation: &SecretString) -> Result<String, ApiError> {
    let mut parsed = url::Url::parse(origin).map_err(|_error| ApiError::internal())?;
    parsed.set_path("/account/yard-login");
    parsed
        .query_pairs_mut()
        .append_pair("continuation", continuation.expose_secret());
    Ok(parsed.into())
}

#[cfg(test)]
fn signature(key: &[u8; 32], payload: &[u8]) -> Result<[u8; 32], ApiError> {
    signature_from(HmacSha256::new_from_slice(key), payload)
}

fn signature_from(
    signer: Result<HmacSha256, hmac::digest::InvalidLength>,
    payload: &[u8],
) -> Result<[u8; 32], ApiError> {
    let mut signer = signer_result(signer)?;
    signer.update(payload);
    Ok(signer.finalize().into_bytes().into())
}

fn issue_payload(result: serde_json::Result<Vec<u8>>) -> Result<Vec<u8>, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(_error) => Err(ApiError::internal()),
    }
}

fn issued_secret(
    result: Result<SecretString, blobyard_core::BlobyardError>,
) -> Result<SecretString, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(_error) => Err(ApiError::internal()),
    }
}

fn decoded_signature(result: Result<Vec<u8>, hex::FromHexError>) -> Result<Vec<u8>, ()> {
    result.map_err(|_error| ())
}

fn verifier_result(
    result: Result<HmacSha256, hmac::digest::InvalidLength>,
) -> Result<HmacSha256, ()> {
    result.map_err(|_error| ())
}

fn canonical_payload(result: serde_json::Result<Vec<u8>>) -> Result<Vec<u8>, ()> {
    result.map_err(|_error| ())
}

const fn signer_result(
    result: Result<HmacSha256, hmac::digest::InvalidLength>,
) -> Result<HmacSha256, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(_error) => Err(ApiError::internal()),
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_host_label(value: &str) -> bool {
    value.contains('-') && blobyard_core::is_valid_dns_label(value)
}

#[cfg(test)]
#[path = "yard_session_contracts_tests.rs"]
mod tests;
