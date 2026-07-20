#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::{ObjectVersionRecord, ReservationStrategy, UploadState};
use rusqlite::Connection;

fn oversized_record() -> UploadReservationRecord {
    UploadReservationRecord {
        id: "upload".to_owned(),
        version: ObjectVersionRecord {
            id: "version".to_owned(),
            project_id: "project".to_owned(),
            object_path: "inbox/file.bin".to_owned(),
            version: 1,
            storage_key: "objects/version".to_owned(),
            state: UploadState::Pending,
            size: None,
            checksum: None,
            created_at_ms: 1,
            source: ObjectSource::Inbox,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        },
        filename: "file.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size: u64::MAX,
        expected_checksum: "a".repeat(64),
        expires_at_ms: 2,
        state: ReservationState::Uploaded,
        strategy: ReservationStrategy::Single,
        part_size: None,
        part_count: None,
        provider_upload_id: None,
    }
}

#[test]
fn inbox_capacity_helpers_reject_unrepresentable_sizes_before_sql() {
    let mut connection = Connection::open_in_memory().expect("database");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        reserve_capacity(&transaction, "inbox", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        complete_capacity(&transaction, "inbox", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        release_capacity(&transaction, "inbox", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        complete_version(&transaction, &oversized_record()),
        Err(RepositoryError::InvalidInput)
    );
}
