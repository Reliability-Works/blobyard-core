#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Fail-closed coverage for the portable storage conformance harness.

/// Failure-injection adapters for storage conformance tests.
pub mod conformance_failure_support;

use blobyard_contract::StorageError;
use blobyard_storage_filesystem::FilesystemStorage;
use conformance_failure_support::{Corrupting, Corruption, Faulting};

#[test]
fn storage_conformance_propagates_each_adapter_failure() {
    let successful_index = (0..128).find(|&failure_index| {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let storage = FilesystemStorage::open(temporary.path()).expect("storage");
        blobyard_testkit::storage_conformance(&Faulting::new(&storage, failure_index)).is_ok()
    });
    assert!(successful_index.is_some(), "conformance must terminate");
    assert_ne!(
        successful_index,
        Some(0),
        "conformance must exercise operations"
    );
}

#[test]
fn storage_conformance_failure_class_is_stable() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    assert_eq!(
        blobyard_testkit::storage_conformance(&Faulting::new(&storage, 0)),
        Err(StorageError::Unavailable)
    );
}

#[test]
fn storage_conformance_rejects_each_inconsistent_result() {
    for corruption in [
        Corruption::MetadataSize,
        Corruption::MetadataChecksum,
        Corruption::MetadataHead,
        Corruption::FullReadBytes,
        Corruption::FullReadFailure,
        Corruption::RangeReadBytes,
        Corruption::RepeatPutError,
        Corruption::IntegrityPutError,
        Corruption::IntegrityHeadError,
        Corruption::DeletedHeadError,
        Corruption::EmptySize,
        Corruption::EmptyReadBytes,
        Corruption::MultipartSize,
        Corruption::MultipartReadBytes,
        Corruption::RepeatedAbortError,
    ] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let storage = FilesystemStorage::open(temporary.path()).expect("storage");
        assert_eq!(
            blobyard_testkit::storage_conformance(&Corrupting::new(&storage, corruption)),
            Err(StorageError::Unavailable),
            "{corruption:?}"
        );
    }
}
