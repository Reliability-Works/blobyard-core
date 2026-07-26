use crate::error::ApiError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_contract::YARD_CONTINUATION_LIFETIME_MS;
use blobyard_core::SecretString;
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
    let payload = internal_result(serialize_claims(&claims))?;
    let signature = signature(key, &payload)?;
    internal_result(SecretString::new(format!(
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
    let supplied_signature = invalid_result(decode_signature(signature_hex))?;
    let mut verifier = invalid_result(mac(key))?;
    verifier.update(&payload);
    verifier
        .verify_slice(&supplied_signature)
        .map_err(|_error| ())?;

    let claims: ContinuationClaims = serde_json::from_slice(&payload).map_err(|_error| ())?;
    let canonical = invalid_result(serialize_claims(&claims))?;
    validate_claims(claims, &payload, now_ms, &canonical)
}

fn validate_claims(
    mut claims: ContinuationClaims,
    payload: &[u8],
    now_ms: u64,
    canonical: &[u8],
) -> Result<ContinuationClaims, ()> {
    if canonical != payload
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
    crate::yards::public_host_label(origin, authority)
}

pub(crate) fn yard_url(origin: &str, host_label: &str) -> Result<String, ApiError> {
    crate::yards::web_yard_url(origin, host_label)
}

pub(crate) fn login_url(origin: &str, continuation: &SecretString) -> Result<String, ApiError> {
    let mut parsed = url::Url::parse(origin).map_err(|_error| ApiError::internal())?;
    parsed.set_path("/account/yard-login");
    parsed
        .query_pairs_mut()
        .append_pair("continuation", continuation.expose_secret());
    Ok(parsed.into())
}

fn signature(key: &[u8; 32], payload: &[u8]) -> Result<[u8; 32], ApiError> {
    let mut signer = internal_result(mac(key))?;
    signer.update(payload);
    Ok(signer.finalize().into_bytes().into())
}

fn serialize_claims(claims: &ContinuationClaims) -> Result<Vec<u8>, serde_json::Error> {
    #[cfg(test)]
    if fault_is(ContractFault::Serialization) {
        return serde_json::from_slice(b"{");
    }
    serde_json::to_vec(claims)
}

fn decode_signature(value: &str) -> Result<Vec<u8>, hex::FromHexError> {
    #[cfg(test)]
    if fault_is(ContractFault::SignatureDecode) {
        return hex::decode("x");
    }
    hex::decode(value)
}

fn mac(key: &[u8; 32]) -> Result<HmacSha256, hmac::digest::InvalidLength> {
    #[cfg(test)]
    if fault_is(ContractFault::Mac) {
        return Err(hmac::digest::InvalidLength);
    }
    HmacSha256::new_from_slice(key)
}

fn invalid_result<T, E>(result: Result<T, E>) -> Result<T, ()> {
    result.map_err(|_error| ())
}

fn internal_result<T, E>(result: Result<T, E>) -> Result<T, ApiError> {
    result.map_err(|_error| ApiError::internal())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum ContractFault {
    Mac,
    Serialization,
    SignatureDecode,
}

#[cfg(test)]
thread_local! {
    static CONTRACT_FAULT: std::cell::Cell<Option<ContractFault>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
struct FaultGuard;

#[cfg(test)]
impl Drop for FaultGuard {
    fn drop(&mut self) {
        CONTRACT_FAULT.with(|slot| slot.set(None));
    }
}

#[cfg(test)]
fn activate(fault: ContractFault) -> FaultGuard {
    CONTRACT_FAULT.with(|slot| slot.set(Some(fault)));
    FaultGuard
}

#[cfg(test)]
fn fault_is(fault: ContractFault) -> bool {
    CONTRACT_FAULT.with(|slot| slot.get() == Some(fault))
}

fn valid_host_label(value: &str) -> bool {
    value.contains('-') && blobyard_core::is_valid_dns_label(value)
}

#[cfg(test)]
#[path = "yard_session_contracts_tests.rs"]
mod tests;
