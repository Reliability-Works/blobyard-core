#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    ReconciliationRepository, add_size, encode, expected_metadata, hash_reader_from,
    inspect_complete, observe, reconcile,
    report::{ObservedObject, ReconciliationReport},
};
use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectSource, ObjectStorage,
    ObjectStorageInventory, ObjectVersionRecord, RepositoryError, StorageError, StorageKey,
    StorageMetadata, StorageRead, UploadState,
};
use std::io::Read;

use crate::storage_multipart_macro::storage_multipart_error;
use crate::storage_put_macro::storage_put_error;

#[derive(Clone, Copy, Debug)]
enum ReadBehavior {
    Error(StorageError),
    MetadataMismatch,
    ReaderFailure,
}

#[derive(Debug)]
struct InspectStorage(ReadBehavior);

impl ObjectStorage for InspectStorage {
    storage_put_error!(StorageError::Unavailable);

    fn get(
        &self,
        _key: &StorageKey,
        _range: Option<ByteRange>,
    ) -> Result<StorageRead, StorageError> {
        match self.0 {
            ReadBehavior::Error(error) => Err(error),
            ReadBehavior::MetadataMismatch => {
                Ok(storage_read(Box::new(std::io::Cursor::new(b"data"))))
            }
            ReadBehavior::ReaderFailure => {
                Ok(storage_read(Box::new(crate::test_support::FailingReader)))
            }
        }
    }

    fn head(&self, _key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        Err(StorageError::Unavailable)
    }

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        Err(StorageError::Unavailable)
    }

    storage_multipart_error!(StorageError::Unavailable);
}

impl ObjectStorageInventory for InspectStorage {
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError> {
        Ok(vec![key()])
    }
}

#[derive(Clone, Copy, Debug)]
enum RepositoryBehavior {
    SchemaFailure,
    CompleteRecord,
}

impl ReconciliationRepository for RepositoryBehavior {
    fn schema_version(&self) -> Result<u32, RepositoryError> {
        match self {
            Self::SchemaFailure => Err(RepositoryError::Unavailable),
            Self::CompleteRecord => Ok(15),
        }
    }

    fn list_object_versions(&self) -> Result<Vec<ObjectVersionRecord>, RepositoryError> {
        match self {
            Self::SchemaFailure => Ok(Vec::new()),
            Self::CompleteRecord => Ok(vec![record()]),
        }
    }
}

#[test]
fn invalid_integrity_metadata_is_reported_without_reading_storage() {
    let mut report = report();
    let mut missing_size = record();
    missing_size.size = None;
    assert!(expected_metadata(&missing_size, &mut report).is_none());

    let mut invalid_checksum = record();
    invalid_checksum.checksum = Some("not-a-checksum".to_owned());
    assert!(expected_metadata(&invalid_checksum, &mut report).is_none());

    assert_eq!(report.invalid_metadata.len(), 2);
    let encoded: serde_json::Value =
        serde_json::from_str(&encode(&report).expect("encode report")).expect("report JSON");
    assert_eq!(
        encoded["invalidMetadata"][0]["reason"],
        "missing_integrity_metadata"
    );
    assert_eq!(
        encoded["invalidMetadata"][1]["reason"],
        "missing_integrity_metadata"
    );
}

#[test]
fn disappearing_and_unavailable_objects_fail_closed() {
    let key = key();
    let record = record();
    let expected = observed();
    let mut report = report();

    inspect_complete(
        &InspectStorage(ReadBehavior::Error(StorageError::NotFound)),
        &key,
        &record,
        &expected,
        &mut report,
    )
    .expect("missing object becomes a finding");
    let encoded: serde_json::Value =
        serde_json::from_str(&encode(&report).expect("encode report")).expect("report JSON");
    assert_eq!(
        encoded["missingBytes"][0]["reason"],
        "physical_object_absent"
    );

    assert_eq!(
        inspect_complete(
            &InspectStorage(ReadBehavior::Error(StorageError::Unavailable)),
            &key,
            &record,
            &expected,
            &mut report,
        ),
        Err(crate::ServerError::Storage)
    );
}

#[test]
fn metadata_disagreement_and_reader_failure_are_integrity_failures() {
    let key = key();
    assert_eq!(
        observe(&InspectStorage(ReadBehavior::MetadataMismatch), &key).err(),
        Some(StorageError::IntegrityMismatch)
    );
    assert_eq!(
        observe(&InspectStorage(ReadBehavior::ReaderFailure), &key).err(),
        Some(StorageError::Unavailable)
    );
}

#[test]
fn reconciliation_propagates_schema_storage_and_size_failures() {
    assert_eq!(
        reconcile(
            &RepositoryBehavior::SchemaFailure,
            &InspectStorage(ReadBehavior::Error(StorageError::NotFound)),
        )
        .err(),
        Some(crate::ServerError::Repository(RepositoryError::Unavailable))
    );
    assert_eq!(
        reconcile(
            &RepositoryBehavior::CompleteRecord,
            &InspectStorage(ReadBehavior::Error(StorageError::Unavailable)),
        )
        .err(),
        Some(crate::ServerError::Storage)
    );
    assert_eq!(add_size(u64::MAX, 1), Err(StorageError::IntegrityMismatch));
    assert_eq!(
        hash_reader_from(
            &mut std::io::Cursor::new([1_u8]),
            &mut sha2::Sha256::default(),
            u64::MAX,
        ),
        Err(StorageError::IntegrityMismatch)
    );
}

fn storage_read(reader: Box<dyn Read + Send>) -> StorageRead {
    StorageRead {
        reader,
        metadata: StorageMetadata {
            size: 4,
            checksum: ObjectChecksum::new("00".repeat(32)).expect("checksum"),
        },
        range: ByteRange { start: 0, end: 4 },
    }
}

fn report() -> ReconciliationReport {
    ReconciliationReport::new(15, 1, 1)
}

fn key() -> StorageKey {
    StorageKey::new("object-key").expect("key")
}

fn observed() -> ObservedObject {
    ObservedObject {
        size: 4,
        checksum: ObjectChecksum::new("00".repeat(32)).expect("checksum"),
    }
}

fn record() -> ObjectVersionRecord {
    ObjectVersionRecord {
        id: "version".to_owned(),
        project_id: "project".to_owned(),
        object_path: "object.bin".to_owned(),
        version: 1,
        storage_key: "object-key".to_owned(),
        state: UploadState::Complete,
        size: Some(4),
        checksum: Some("00".repeat(32)),
        created_at_ms: 0,
        source: ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}
