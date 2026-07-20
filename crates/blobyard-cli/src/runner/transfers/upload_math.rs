use super::file_facts::FileFacts;
use super::resume::ResumeState;
use blobyard_api_client::{RequestUploadResponse, UploadPartGrant};
use blobyard_core::{BlobyardError, ErrorCode};

const MAX_PARTS: u64 = 10_000;
const MIN_PART_SIZE: u64 = 8 * 1024 * 1024;

pub(super) fn multipart_state(
    reservation: &RequestUploadResponse,
    facts: &FileFacts,
) -> Result<ResumeState, BlobyardError> {
    let Some(part_size) = reservation.part_size_bytes else {
        return Err(contract_error());
    };
    total_parts(facts.size_bytes, part_size)?;
    Ok(ResumeState::new(
        reservation.upload_id.clone(),
        facts.fingerprint.clone(),
        part_size,
    ))
}

pub(super) fn total_parts(size: u64, part_size: u64) -> Result<u32, BlobyardError> {
    if part_size < MIN_PART_SIZE {
        return Err(contract_error());
    }
    let count = size.div_ceil(part_size);
    if count == 0 || count > MAX_PARTS {
        return Err(contract_error());
    }
    let bytes = count.to_le_bytes();
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(super) fn part_range(part_number: u32, part_size: u64, total_size: u64) -> (u64, u64) {
    let offset = u64::from(part_number - 1) * part_size;
    (offset, (total_size - offset).min(part_size))
}

pub(super) fn validate_grants(
    requested: &[u32],
    mut grants: Vec<UploadPartGrant>,
) -> Result<Vec<UploadPartGrant>, BlobyardError> {
    grants.sort_by_key(|grant| grant.part_number);
    let exact = grants.len() == requested.len()
        && grants
            .iter()
            .map(|grant| grant.part_number)
            .eq(requested.iter().copied());
    if exact {
        Ok(grants)
    } else {
        Err(contract_error())
    }
}

pub(super) fn contract_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::ProviderUnavailable,
        "Blobyard received an invalid storage grant. Try the upload again.",
    )
}
