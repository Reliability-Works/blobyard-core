use blobyard_core::{BlobyardError, ErrorCode, hex_digest};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::UNIX_EPOCH;
use tokio::io::AsyncReadExt;

const READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileFacts {
    pub(super) size_bytes: u64,
    pub(super) checksum_sha256: String,
    pub(super) content_type: String,
    pub(super) fingerprint: String,
}

pub(super) async fn inspect(path: &Path) -> Result<FileFacts, BlobyardError> {
    inspect_with_hook(path, noop).await
}

pub(super) async fn inspect_with_hook(
    path: &Path,
    after_metadata: fn(&Path),
) -> Result<FileFacts, BlobyardError> {
    let metadata = tokio::fs::metadata(path).await.map_err(read_error)?;
    if !metadata.is_file() {
        return Err(read_error(()));
    }
    after_metadata(path);
    let mut file = tokio::fs::File::open(path).await.map_err(read_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    let mut measured = 0_u64;
    loop {
        let count = file.read(&mut buffer).await.map_err(read_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        measured = measured.saturating_add(count as u64);
    }
    validate_measured(measured, metadata.len())?;
    let checksum_sha256 = hex_digest(hasher.finalize().as_slice());
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0_u128, |duration| duration.as_nanos());
    let fingerprint = fingerprint(metadata.len(), modified, &checksum_sha256);
    Ok(FileFacts {
        size_bytes: metadata.len(),
        checksum_sha256,
        content_type: content_type(path).to_owned(),
        fingerprint,
    })
}

const fn noop(_path: &Path) {}

pub(super) fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("txt" | "log" | "md") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

pub(super) fn fingerprint(size: u64, modified_nanos: u128, checksum: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(size.to_le_bytes());
    hasher.update(modified_nanos.to_le_bytes());
    hasher.update(checksum.as_bytes());
    hex_digest(hasher.finalize().as_slice())
}

fn read_error<E>(_error: E) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't read the upload source. Check its permissions and try again.",
    )
}

pub(super) fn validate_measured(measured: u64, expected: u64) -> Result<(), BlobyardError> {
    if measured == expected {
        Ok(())
    } else {
        Err(read_error(()))
    }
}
