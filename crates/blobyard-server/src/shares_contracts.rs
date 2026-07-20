use crate::{error::ApiError, transfer_grants as grants};
use blobyard_api_client::{ShareNotificationStatus, ShareSummary};
use blobyard_contract::{ShareRecord, ShareStatus, ShareTarget};
use blobyard_core::SecretString;

const DEFAULT_SHARE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAXIMUM_SHARE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

pub(super) fn share_expiry(now: u64, duration: Option<&str>) -> Result<u64, ApiError> {
    crate::expiry::bounded_expiry(now, duration, DEFAULT_SHARE_TTL_MS, MAXIMUM_SHARE_TTL_MS)
}

pub(super) fn notification_status(
    value: Option<&str>,
) -> Result<ShareNotificationStatus, ApiError> {
    let Some(value) = value else {
        return Ok(ShareNotificationStatus::NotRequested);
    };
    let normalized = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = normalized.split_once('@') else {
        return Err(ApiError::invalid_request());
    };
    if normalized.len() > 254
        || local.is_empty()
        || domain.split_once('.').is_none()
        || normalized.chars().any(char::is_whitespace)
        || normalized.chars().any(char::is_control)
    {
        return Err(ApiError::invalid_request());
    }
    Ok(ShareNotificationStatus::Failed)
}

pub(super) fn share_url(origin: &str, capability: &SecretString) -> Result<SecretString, ApiError> {
    SecretString::new(format!("{origin}/s/{}", capability.expose_secret()))
        .map_err(|_error| ApiError::internal())
}

pub(super) fn share_download_expiry(
    now: u64,
    lifetime: u64,
    share_expiry: u64,
) -> Result<u64, ApiError> {
    now.checked_add(lifetime)
        .map(|expiry| expiry.min(share_expiry))
        .ok_or_else(ApiError::internal)
}

pub(super) fn share_summary(value: ShareRecord, now: u64) -> Result<ShareSummary, ApiError> {
    let status = if value.status == ShareStatus::Revoked {
        "revoked"
    } else if value.expires_at_ms <= now {
        "expired"
    } else if value.version_id.is_none() {
        "unavailable"
    } else {
        value.status.as_str()
    };
    Ok(ShareSummary {
        id: value.id,
        expires_at: grants::format_expiry(value.expires_at_ms)?,
        status: status.to_owned(),
        consumed_count: value.consumed_count,
        maximum_downloads: value.maximum_downloads,
    })
}

pub(super) fn share_page_html(value: &ShareTarget, action: &str) -> Result<String, ApiError> {
    let filename = escape_html(&value.object.filename);
    let action = escape_html(action);
    let expires_at = grants::format_expiry(value.share.expires_at_ms)?;
    let available = value.share.status == ShareStatus::Active;
    let control = if available {
        format!(
            "<form action=\"{action}\" method=\"post\"><button type=\"submit\">Download</button></form>"
        )
    } else {
        "<p>Download limit reached.</p>".to_owned()
    };
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Shared file | Blob Yard</title></head><body><main><p>Shared through Blob Yard</p><h1>{filename}</h1><p>{} bytes. Available until {} UTC.</p>{control}</main></body></html>",
        value.object.version.size.ok_or_else(ApiError::not_found)?,
        escape_html(&expires_at)
    ))
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub(super) fn content_type_class(value: &str) -> &'static str {
    match value.split_once('/').map(|(family, _rest)| family) {
        Some("audio") => return "audio",
        Some("image") => return "image",
        Some("text") => return "text",
        Some("video") => return "video",
        _ => {}
    }
    if matches!(value, "application/json" | "application/pdf") || value.ends_with("+json") {
        "document"
    } else if matches!(
        value,
        "application/gzip"
            | "application/x-7z-compressed"
            | "application/x-tar"
            | "application/zip"
    ) {
        "archive"
    } else {
        "binary"
    }
}
