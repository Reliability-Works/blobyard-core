use crate::{
    error::ApiError,
    site_contracts::{self, PublicPathError, SiteManifestError},
    transfer_grants as grants,
};
use blobyard_contract::{NewPreviewFile, PreviewRecord, PreviewStatus, StoredObjectRecord};
use blobyard_core::{SecretString, WebYardOrigin};

pub(super) const PREVIEW_MANIFEST_ROOT: &str = ".blobyard-preview";
const DEFAULT_PREVIEW_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAXIMUM_PREVIEW_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub(super) fn preview_expiry(now: u64, duration: Option<&str>) -> Result<u64, ApiError> {
    crate::expiry::bounded_expiry(
        now,
        duration,
        DEFAULT_PREVIEW_TTL_MS,
        MAXIMUM_PREVIEW_TTL_MS,
    )
}

pub(super) fn manifest_root(manifest_id: &str) -> Result<String, ApiError> {
    let valid_length = (16..=128).contains(&manifest_id.len());
    let valid_start = manifest_id
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric);
    let valid_tail = manifest_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if valid_length && valid_start && valid_tail {
        Ok(format!("{PREVIEW_MANIFEST_ROOT}/{manifest_id}/"))
    } else {
        Err(ApiError::invalid_request())
    }
}

pub(super) fn snapshot_manifest(
    root: &str,
    objects: Vec<StoredObjectRecord>,
) -> Result<Vec<NewPreviewFile>, ApiError> {
    let objects =
        site_contracts::snapshot_manifest(root, objects, blobyard_contract::is_valid_preview_path)
            .map_err(|error| match error {
                SiteManifestError::Invalid => ApiError::invalid_request(),
                SiteManifestError::Duplicate => ApiError::conflict(),
            })?;
    Ok(objects
        .into_iter()
        .map(|object| NewPreviewFile {
            normalized_path: object.normalized_path,
            version_id: object.version_id,
        })
        .collect())
}

pub(super) fn preview_url(
    origin: &str,
    capability: &SecretString,
) -> Result<SecretString, ApiError> {
    let origin = WebYardOrigin::new(origin).map_err(|_error| ApiError::internal())?;
    origin
        .secret_url_for(capability.expose_secret())
        .map_err(|_error| ApiError::internal())
}

pub(super) fn public_host_capability(origin: &str, authority: &str) -> Option<String> {
    let origin = WebYardOrigin::new(origin).ok()?;
    let suffix = format!(".{}", origin.authority());
    let capability = authority.strip_suffix(&suffix)?;
    valid_host_capability(capability).then(|| capability.to_owned())
}

fn valid_host_capability(value: &str) -> bool {
    value.len() == 52
        && value.bytes().enumerate().all(|(index, byte)| {
            let alphabet = byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte);
            let canonical_tail = index != 51 || b"acegikmoqsuwy246".contains(&byte);
            alphabet && canonical_tail
        })
}

pub(super) fn public_preview_path(path: &str) -> Result<String, ApiError> {
    site_contracts::public_path(
        path,
        "index.html",
        "/index.html",
        blobyard_contract::is_valid_preview_path,
    )
    .map_err(|error| match error {
        PublicPathError::InvalidPath => ApiError::invalid_request(),
        PublicPathError::NotFound => ApiError::not_found(),
    })
}

pub(super) fn status(record: &PreviewRecord, now: u64) -> &'static str {
    if record.status == PreviewStatus::Revoked {
        "revoked"
    } else if record.expires_at_ms <= now {
        "expired"
    } else {
        "active"
    }
}

pub(super) fn formatted_time(value: u64) -> Result<String, ApiError> {
    grants::format_expiry(value)
}
