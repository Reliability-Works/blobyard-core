use crate::{auth::hash, error::ApiError, transfer_grants as grants};
use blobyard_api_client::{InboxMetadata, InboxSummary};
use blobyard_contract::{InboxRecord, InboxStatus};
use blobyard_core::SecretString;
use std::net::SocketAddr;

pub(super) const MAXIMUM_FILES: u64 = 20;
pub(super) const MAXIMUM_BYTES: u64 = 1_073_741_824;
pub(super) const RESOLVE_RATE_LIMIT: u32 = 120;
pub(super) const RATE_WINDOW_MS: u64 = 60_000;
const DEFAULT_INBOX_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAXIMUM_INBOX_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub(super) fn expiry(now: u64, duration: Option<&str>) -> Result<u64, ApiError> {
    crate::expiry::bounded_expiry(now, duration, DEFAULT_INBOX_TTL_MS, MAXIMUM_INBOX_TTL_MS)
}

pub(super) fn inbox_url(origin: &str, capability: &SecretString) -> Result<SecretString, ApiError> {
    SecretString::new(format!("{origin}/i/{}", capability.expose_secret()))
        .map_err(|_error| ApiError::internal())
}

pub(super) fn summary(record: InboxRecord) -> Result<InboxSummary, ApiError> {
    Ok(InboxSummary {
        id: record.id,
        name: record.name,
        expires_at: grants::format_expiry(record.expires_at_ms)?,
        revoked: record.status == InboxStatus::Revoked,
    })
}

pub(super) fn metadata(record: InboxRecord) -> Result<InboxMetadata, ApiError> {
    let used_files = record
        .current_files
        .checked_add(record.reserved_files)
        .ok_or_else(ApiError::internal)?;
    let used_bytes = record
        .current_bytes
        .checked_add(record.reserved_bytes)
        .ok_or_else(ApiError::internal)?;
    Ok(InboxMetadata {
        name: record.name,
        max_files: u32::try_from(record.maximum_files).map_err(|_error| ApiError::internal())?,
        max_bytes: record.maximum_bytes,
        expires_at: grants::format_expiry(record.expires_at_ms)?,
        upload_available: record.status == InboxStatus::Active
            && used_files < record.maximum_files
            && used_bytes < record.maximum_bytes,
    })
}

pub(super) fn peer_fingerprint(peer: Option<SocketAddr>) -> String {
    peer.map_or_else(|| "unavailable".to_owned(), |value| value.ip().to_string())
}

pub(super) fn resolve_rate_key(token_hash: &str, fingerprint: &str) -> String {
    hash(&format!("resolve\0{token_hash}\0{fingerprint}"))
}
