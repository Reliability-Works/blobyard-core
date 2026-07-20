use blobyard_api_client::CompletedPart;
use blobyard_core::hex_digest;
use blobyard_core::{BlobyardError, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

const MIN_PART_SIZE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ResumeState {
    upload_id: String,
    source_fingerprint: String,
    part_size_bytes: u64,
    completed_etags: BTreeMap<u32, String>,
}

impl ResumeState {
    pub(super) const fn new(upload_id: String, fingerprint: String, part_size_bytes: u64) -> Self {
        Self {
            upload_id,
            source_fingerprint: fingerprint,
            part_size_bytes,
            completed_etags: BTreeMap::new(),
        }
    }

    pub(super) fn upload_id(&self) -> &str {
        &self.upload_id
    }

    pub(super) const fn part_size_bytes(&self) -> u64 {
        self.part_size_bytes
    }

    pub(super) fn matches(&self, fingerprint: &str) -> bool {
        self.source_fingerprint == fingerprint
    }

    pub(super) fn retain_server_parts(&mut self, server_parts: &[u32]) {
        self.completed_etags
            .retain(|number, _etag| server_parts.contains(number));
    }

    pub(super) fn record(&mut self, part_number: u32, etag: String) {
        self.completed_etags.insert(part_number, etag);
    }

    pub(super) fn pending(&self, total_parts: u32) -> Vec<u32> {
        (1..=total_parts)
            .filter(|number| !self.completed_etags.contains_key(number))
            .collect()
    }

    pub(super) fn completed_parts(&self) -> Vec<CompletedPart> {
        self.completed_etags
            .iter()
            .map(|(part_number, etag)| CompletedPart {
                part_number: *part_number,
                etag: etag.clone(),
            })
            .collect()
    }

    pub(super) fn completed_bytes(&self, total_size: u64) -> u64 {
        self.completed_etags.keys().fold(0, |total, number| {
            let offset = u64::from(number.saturating_sub(1)).saturating_mul(self.part_size_bytes);
            total.saturating_add(total_size.saturating_sub(offset).min(self.part_size_bytes))
        })
    }

    fn is_valid(&self) -> bool {
        valid_text(&self.upload_id, 128)
            && self.part_size_bytes >= MIN_PART_SIZE_BYTES
            && self.source_fingerprint.len() == 64
            && self
                .source_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && self
                .completed_etags
                .iter()
                .all(|(number, etag)| *number > 0 && valid_text(etag, 1_024))
    }
}

pub(super) fn state_path(source: &Path, logical_path: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(source.as_os_str().as_encoded_bytes());
    hasher.update([0]);
    hasher.update(logical_path.as_bytes());
    let digest = hex_digest(hasher.finalize().as_slice());
    source.with_file_name(format!(".blobyard-resume-{}.json", &digest[..16]))
}

pub(super) fn load(path: &Path) -> Result<Option<ResumeState>, BlobyardError> {
    match std::fs::read(path) {
        Ok(bytes) => {
            validate_permissions(path)?;
            let state = serde_json::from_slice::<ResumeState>(&bytes).map_err(|_| state_error())?;
            state
                .is_valid()
                .then_some(state)
                .map(Some)
                .ok_or_else(state_error)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_error) => Err(state_error()),
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(unix)]
pub(super) fn validate_permissions(path: &Path) -> Result<(), BlobyardError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)
        .map_err(state_error_from)?
        .permissions()
        .mode();
    if mode.trailing_zeros() >= 6 {
        Ok(())
    } else {
        Err(state_error())
    }
}

#[cfg(not(unix))]
pub(super) fn validate_permissions(_path: &Path) -> Result<(), BlobyardError> {
    Ok(())
}

pub(super) fn save(path: &Path, state: &ResumeState) -> Result<(), BlobyardError> {
    save_with_failure(path, state, 0)
}

#[cfg(test)]
pub(super) fn fail_save(
    path: &Path,
    state: &ResumeState,
    failure_step: u8,
) -> Result<(), BlobyardError> {
    save_with_failure(path, state, failure_step)
}

fn save_with_failure(
    path: &Path,
    state: &ResumeState,
    failure_step: u8,
) -> Result<(), BlobyardError> {
    let parent = path.parent().ok_or_else(state_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(state_error_from)?;
    save_step(temporary.write_all(&encoded(state)), failure_step == 1)?;
    save_step(temporary.flush(), failure_step == 2)?;
    save_step(temporary.rewind(), failure_step == 3)?;
    save_step(restrict_permissions(temporary.as_file()), failure_step == 4)?;
    save_step(temporary.as_file().sync_all(), failure_step == 5)?;
    let persisted = if failure_step == 6 {
        synthetic_failure()
    } else {
        match temporary.persist(path) {
            Ok(_file) => Ok(()),
            Err(error) => Err(error.error),
        }
    };
    save_step(persisted, false)?;
    Ok(())
}

fn encoded(state: &ResumeState) -> Vec<u8> {
    let completed = state
        .completed_etags
        .iter()
        .map(|(number, etag)| (number.to_string(), serde_json::Value::String(etag.clone())))
        .collect();
    let mut value = serde_json::Map::new();
    value.insert(
        "uploadId".into(),
        serde_json::Value::String(state.upload_id.clone()),
    );
    value.insert(
        "sourceFingerprint".into(),
        serde_json::Value::String(state.source_fingerprint.clone()),
    );
    value.insert(
        "partSizeBytes".into(),
        serde_json::Value::Number(state.part_size_bytes.into()),
    );
    value.insert(
        "completedEtags".into(),
        serde_json::Value::Object(completed),
    );
    serde_json::Value::Object(value).to_string().into_bytes()
}

fn save_step(result: std::io::Result<()>, fail: bool) -> Result<(), BlobyardError> {
    let result = if fail { synthetic_failure() } else { result };
    result.map_err(state_error_from)
}

fn synthetic_failure() -> std::io::Result<()> {
    Err(std::io::Error::other(
        "synthetic resume persistence failure",
    ))
}

pub(super) fn remove(path: &Path) -> Result<(), BlobyardError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(state_error()),
    }
}

#[cfg(unix)]
fn restrict_permissions(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_file: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}

fn state_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't update secure upload resume state. Check local permissions and try again.",
    )
}

pub(super) fn state_error_from(_error: std::io::Error) -> BlobyardError {
    state_error()
}
