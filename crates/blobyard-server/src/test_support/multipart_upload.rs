use blobyard_contract::{
    NewUploadReservation, ObjectSource, ObjectVersionRecord, ReservationState, ReservationStrategy,
    UploadReservationRecord, UploadState,
};

pub(crate) fn record(
    id: &str,
    expected_size: u64,
    part_size: u64,
    part_count: u32,
    provider_upload_id: Option<&str>,
) -> UploadReservationRecord {
    UploadReservationRecord {
        id: id.to_owned(),
        version: ObjectVersionRecord {
            id: format!("version_{id}"),
            project_id: "project_fixture".to_owned(),
            object_path: "fixture.bin".to_owned(),
            version: 1,
            storage_key: format!("objects/{id}"),
            state: UploadState::Pending,
            size: None,
            checksum: None,
            created_at_ms: 1,
            source: ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        },
        filename: "fixture.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size,
        expected_checksum: "a".repeat(64),
        expires_at_ms: 10,
        state: ReservationState::Requested,
        strategy: ReservationStrategy::Multipart,
        part_size: Some(part_size),
        part_count: Some(part_count),
        provider_upload_id: provider_upload_id.map(str::to_owned),
    }
}

pub(crate) fn reservation(
    upload: &UploadReservationRecord,
    capability_character: char,
    expires_at_ms: u64,
) -> NewUploadReservation {
    NewUploadReservation {
        id: upload.id.clone(),
        project_id: upload.version.project_id.clone(),
        object_path: upload.version.object_path.clone(),
        filename: upload.filename.clone(),
        content_type: upload.content_type.clone(),
        expected_size: upload.expected_size,
        expected_checksum: upload.expected_checksum.clone(),
        storage_key: upload.version.storage_key.clone(),
        capability_hash: capability_character.to_string().repeat(64),
        expires_at_ms,
        created_at_ms: upload.version.created_at_ms,
        source: upload.version.source,
        git_repository: upload.version.git_repository.clone(),
        git_commit: upload.version.git_commit.clone(),
        git_branch: upload.version.git_branch.clone(),
        strategy: upload.strategy,
        part_size: upload.part_size,
        part_count: upload.part_count,
    }
}
