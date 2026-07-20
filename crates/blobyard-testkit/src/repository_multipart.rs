use blobyard_contract::{
    NewUploadPartGrant, RepositoryError, ReservationState, ReservationStrategy, TransferRepository,
    UploadState,
};

pub(super) fn conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let mut upload = super::upload(
        "upload_multipart",
        project_id,
        "multipart/complete.bin",
        '6',
    );
    upload.strategy = ReservationStrategy::Multipart;
    upload.part_size = Some(3);
    upload.part_count = Some(2);
    let reserved = repository.reserve_upload(&upload)?;
    if reserved.strategy != ReservationStrategy::Multipart
        || reserved.part_size != Some(3)
        || reserved.part_count != Some(2)
        || reserved.provider_upload_id.is_some()
    {
        return Err(RepositoryError::Unavailable);
    }
    let attached = repository.attach_multipart(&upload.id, "provider-fixture")?;
    if attached.provider_upload_id.as_deref() != Some("provider-fixture")
        || repository.attach_multipart(&upload.id, "provider-fixture")? != attached
        || repository.attach_multipart(&upload.id, "provider-other")
            != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    let grants = [part(1, 3, '7'), part(2, 2, '8')];
    let issued = repository.issue_upload_parts(&grants)?;
    if issued.len() != 2 || issued[0].part_number != 1 || issued[1].part_number != 2 {
        return Err(RepositoryError::Unavailable);
    }
    let (resolved_upload, resolved_part) =
        repository.upload_part_by_capability(&grants[0].capability_hash, 999)?;
    if resolved_upload.id != upload.id
        || resolved_part != issued[0]
        || repository.upload_part_by_capability(&grants[0].capability_hash, 1_000)
            != Err(RepositoryError::NotFound)
        || repository.record_uploaded_part(&upload.id, 1, 2, &super::hash('9'), Some("tag-one"))
            != Err(RepositoryError::InvalidInput)
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.record_uploaded_part(&upload.id, 1, 3, &super::hash('9'), Some("tag-one"))?;
    repository.record_uploaded_part(&upload.id, 2, 2, &super::hash('a'), Some("tag-two"))?;
    let listed = repository.list_upload_parts(&upload.id)?;
    if listed.len() != 2
        || listed[0].received_checksum.as_deref() != Some(super::hash('9').as_str())
        || listed[1].received_checksum.as_deref() != Some(super::hash('a').as_str())
        || listed[0].provider_tag.as_deref() != Some("tag-one")
        || listed[1].provider_tag.as_deref() != Some("tag-two")
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.record_uploaded_bytes(&upload.id, 5, super::hello_checksum())?;
    let completed = repository.complete_upload(&upload.id)?;
    if completed.state != UploadState::Complete {
        return Err(RepositoryError::Unavailable);
    }
    abort_conformance(repository, project_id)
}

fn abort_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let mut upload = super::upload(
        "upload_multipart_abort",
        project_id,
        "multipart/abort.bin",
        'b',
    );
    upload.strategy = ReservationStrategy::Multipart;
    upload.part_size = Some(3);
    upload.part_count = Some(2);
    repository.reserve_upload(&upload)?;
    repository.attach_multipart(&upload.id, "provider-abort")?;
    repository.issue_upload_parts(&[
        NewUploadPartGrant {
            upload_id: upload.id.clone(),
            ..part(1, 3, 'c')
        },
        NewUploadPartGrant {
            upload_id: upload.id.clone(),
            ..part(2, 2, 'd')
        },
    ])?;
    let prior = repository.abort_upload(&upload.id)?;
    if prior.state != ReservationState::Requested
        || repository.upload_by_id(&upload.id)?.state != ReservationState::Aborted
        || !repository.list_upload_parts(&upload.id)?.is_empty()
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn part(number: u32, size: u64, character: char) -> NewUploadPartGrant {
    NewUploadPartGrant {
        upload_id: "upload_multipart".to_owned(),
        part_number: number,
        expected_size: size,
        capability_hash: super::hash(character),
        expires_at_ms: 1_000,
    }
}
