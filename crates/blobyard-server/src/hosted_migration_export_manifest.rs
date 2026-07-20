use super::HostedMigrationError;
use serde::Deserialize;
use std::collections::BTreeSet;

pub(super) const MAX_EXPORT_PARTS: usize = 10_000;
pub(super) const MAX_EXPORT_PART_BYTES: u64 = 8 * 1_024 * 1_024;
const MAX_REQUIRED_EXPORT_BYTES: u64 = 128 * 1_024 * 1_024;
pub(super) const REQUIRED_DATASETS: &[&str] = &[
    "workspace",
    "projects",
    "objects",
    "versions",
    "shares",
    "retention_policies",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportIndexFile {
    dataset: String,
    records: Vec<ExportIndex>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportIndex {
    format: String,
    parts: Vec<ExportPart>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExportPart {
    pub(super) byte_size: u64,
    pub(super) checksum_sha256: String,
    pub(super) dataset: String,
    pub(super) part_number: u32,
}

pub(super) fn parse_index(
    bytes: &[u8],
    artifact_count: usize,
) -> Result<Vec<ExportPart>, HostedMigrationError> {
    let file = serde_json::from_slice::<ExportIndexFile>(bytes)
        .map_err(|_error| HostedMigrationError::InvalidExport)?;
    if file.dataset != "complete" {
        return Err(HostedMigrationError::InvalidExport);
    }
    let [index] = file.records.as_slice() else {
        return Err(HostedMigrationError::InvalidExport);
    };
    if index.format != "Blob Yard account export v1"
        || index.parts.len().saturating_add(1) != artifact_count
        || index.parts.len() > MAX_EXPORT_PARTS
    {
        return Err(HostedMigrationError::InvalidExport);
    }
    validate_parts(&index.parts)?;
    Ok(index.parts.clone())
}

pub(super) fn validate_parts(parts: &[ExportPart]) -> Result<(), HostedMigrationError> {
    let mut numbers = BTreeSet::new();
    let mut required_bytes = 0_u64;
    for part in parts {
        if part.part_number == 0
            || !numbers.insert(part.part_number)
            || part.byte_size > MAX_EXPORT_PART_BYTES
            || !valid_checksum(&part.checksum_sha256)
        {
            return Err(HostedMigrationError::InvalidExport);
        }
        if REQUIRED_DATASETS.contains(&part.dataset.as_str()) {
            required_bytes = required_bytes.saturating_add(part.byte_size);
        }
    }
    if required_bytes > MAX_REQUIRED_EXPORT_BYTES {
        Err(HostedMigrationError::InvalidExport)
    } else {
        Ok(())
    }
}

pub(super) fn valid_checksum(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
