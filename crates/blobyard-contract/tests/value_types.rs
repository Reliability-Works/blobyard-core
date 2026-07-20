#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Provider-independent storage value type guards.

use blobyard_contract::{
    ByteRange, ObjectChecksum, ObjectSource, RepositoryError, ReservationState, StorageError,
    StorageKey, UploadState,
};

#[test]
fn storage_keys_reject_escape_and_ambiguous_components() {
    for invalid in ["", "/absolute", "../escape", "a/../b", "a//b", "a\\b", "a/"] {
        assert_eq!(StorageKey::new(invalid), Err(StorageError::InvalidInput));
    }
    assert_eq!(
        StorageKey::new("objects/releases/app.zip")
            .expect("valid key")
            .as_str(),
        "objects/releases/app.zip"
    );
}

#[test]
fn checksums_and_ranges_are_canonical() {
    let checksum = "a".repeat(64);
    assert_eq!(
        ObjectChecksum::new(checksum.clone())
            .expect("checksum")
            .as_str(),
        checksum
    );
    for invalid in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        assert_eq!(
            ObjectChecksum::new(invalid),
            Err(StorageError::InvalidInput)
        );
    }
    assert_eq!(ByteRange::new(0, 0).expect("empty object").end, 0);
    assert_eq!(ByteRange::new(2, 1), Err(StorageError::InvalidInput));
}

#[test]
fn exact_sha256_digests_build_valid_checksums_without_revalidation() {
    let checksum = ObjectChecksum::from_sha256_digest([0xab; 32]);
    assert_eq!(checksum.as_str(), "ab".repeat(32));
}

#[test]
fn stable_errors_render_their_public_failure_classes() {
    let repository = [
        (RepositoryError::NotFound, "metadata record not found"),
        (RepositoryError::Conflict, "metadata conflict"),
        (RepositoryError::InvalidInput, "invalid metadata input"),
        (
            RepositoryError::SchemaTooNew,
            "metadata schema is newer than this runtime",
        ),
        (
            RepositoryError::Unavailable,
            "metadata repository unavailable",
        ),
    ];
    for (error, message) in repository {
        assert_eq!(error.to_string(), message);
    }

    let storage = [
        (StorageError::NotFound, "stored object not found"),
        (StorageError::Conflict, "storage conflict"),
        (StorageError::InvalidInput, "invalid storage input"),
        (
            StorageError::IntegrityMismatch,
            "stored object integrity mismatch",
        ),
        (StorageError::Unavailable, "storage provider unavailable"),
    ];
    for (error, message) in storage {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn persisted_states_round_trip_and_reject_unknown_values() {
    for state in [
        UploadState::Pending,
        UploadState::Complete,
        UploadState::Aborted,
    ] {
        assert_eq!(UploadState::parse(state.as_str()), Some(state));
    }
    assert_eq!(UploadState::parse("uploaded"), None);

    for state in [
        ReservationState::Requested,
        ReservationState::Uploaded,
        ReservationState::Complete,
        ReservationState::Aborted,
    ] {
        assert_eq!(ReservationState::parse(state.as_str()), Some(state));
    }
    assert_eq!(ReservationState::parse("pending"), None);

    for source in [
        ObjectSource::Ci,
        ObjectSource::Cli,
        ObjectSource::Inbox,
        ObjectSource::Preview,
        ObjectSource::Web,
    ] {
        assert_eq!(ObjectSource::parse(source.as_str()), Some(source));
    }
    assert_eq!(ObjectSource::parse("unknown"), None);
}
