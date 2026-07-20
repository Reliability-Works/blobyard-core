use super::{PeerFingerprint, operations};
use crate::{api::AppState, error::ApiError};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Response, StatusCode, header},
};
use blobyard_api_client::ResolveInboxQuery;
use blobyard_core::SecretString;

const SCRIPT: &str = include_str!("../assets/inbox-upload.js");

pub(super) async fn open(
    State(state): State<AppState>,
    PeerFingerprint(fingerprint): PeerFingerprint,
    Path(token): Path<String>,
) -> Result<Response<Body>, ApiError> {
    let token = ApiError::not_found_result(SecretString::new(token))?;
    let metadata = operations::resolve_metadata_at(
        &state,
        &ResolveInboxQuery { token },
        crate::transfer_grants::now_ms(),
        &fingerprint,
    )?;
    page_response(page(&metadata))
}

pub(super) async fn script() -> Result<Response<Body>, ApiError> {
    ApiError::internal_result(
        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            )
            .header(header::CACHE_CONTROL, "public, max-age=300")
            .header("x-content-type-options", "nosniff")
            .body(Body::from(SCRIPT)),
    )
}

fn page_response(html: String) -> Result<Response<Body>, ApiError> {
    crate::response::secure_html(
        html,
        "default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'unsafe-inline'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'",
    )
}

fn page(metadata: &blobyard_api_client::InboxMetadata) -> String {
    let disabled = if metadata.upload_available {
        ""
    } else {
        " disabled"
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Guest upload | Blob Yard</title><style>{}</style><script defer src=\"/assets/inbox-upload.js\"></script></head><body><main data-inbox data-max-bytes=\"{}\"><p class=\"brand\">BLOB YARD</p><p class=\"eyebrow\">GUEST UPLOAD</p><h1>{}</h1><p>Send one file securely. This inbox accepts up to {} files and {} bytes, and is available until {}.</p><form><label for=\"inbox-file\">File</label><input id=\"inbox-file\" name=\"file\" type=\"file\" required{}><button type=\"submit\"{}>Upload file</button></form><p data-status role=\"status\" aria-live=\"polite\"></p></main></body></html>",
        STYLE,
        metadata.max_bytes,
        escape(&metadata.name),
        metadata.max_files,
        metadata.max_bytes,
        escape(&metadata.expires_at),
        disabled,
        disabled,
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = "*{box-sizing:border-box}body{margin:0;background:#f4f6f1;color:#10130f;font:16px system-ui,sans-serif}main{width:min(42rem,calc(100% - 2rem));margin:10vh auto;padding:2rem;border:1px solid #c8cdc5;background:#fff}.brand,.eyebrow{font:700 .75rem ui-monospace,monospace;letter-spacing:.12em}.eyebrow{color:#4c6500;margin-top:3rem}h1{font-size:clamp(2rem,8vw,4rem);line-height:.95}form{display:grid;gap:1rem;margin-top:2rem}input{padding:1rem;border:1px solid #747a70}button{width:max-content;padding:.8rem 1rem;border:0;background:#b6ff00;color:#10130f;font-weight:700}button:disabled,input:disabled{opacity:.5}[data-status][data-error]{color:#9e251b}";

#[cfg(test)]
#[path = "inbox_browser_tests.rs"]
mod tests;
