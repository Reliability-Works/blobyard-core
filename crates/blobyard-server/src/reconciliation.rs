use crate::{RuntimeStorage, ServerError, StorageConfiguration};
use blobyard_contract::{
    MetadataRepository, MetadataRepositoryInventory, ObjectChecksum, ObjectStorage,
    ObjectVersionRecord, RepositoryError, StorageError, StorageKey, UploadState,
};
use blobyard_repository_sqlite::SqliteRepository;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

#[path = "reconciliation_report.rs"]
mod report;
use report::{
    IntegrityFinding, MetadataFinding, ObservedObject, PhysicalFinding, ReconciliationReport,
};

/// Produces a deterministic read-only reconciliation report for one standalone installation.
///
/// # Errors
///
/// Returns a stable repository, configuration, storage, or serialization failure. Provider errors
/// fail closed rather than producing a false clean report.
pub fn reconcile_data_directory(
    data_directory: &Path,
    storage_configuration: &StorageConfiguration,
) -> Result<String, ServerError> {
    let repository = SqliteRepository::open(&data_directory.join("metadata.sqlite3"))?;
    let storage = storage_configuration.open(data_directory)?;
    reconcile(&repository, storage.as_ref()).and_then(|report| encode(&report))
}

fn reconcile(
    repository: &dyn ReconciliationRepository,
    storage: &dyn RuntimeStorage,
) -> Result<ReconciliationReport, ServerError> {
    let records = repository.list_object_versions()?;
    let physical_keys = storage.list_object_keys().map_err(storage_error)?;
    let mut report = ReconciliationReport::new(
        repository.schema_version()?,
        records.len(),
        physical_keys.len(),
    );
    let mut metadata = BTreeMap::new();
    for record in records {
        match StorageKey::new(record.storage_key.clone()) {
            Ok(key) => {
                metadata.insert(key, record);
            }
            Err(_error) => report
                .invalid_metadata
                .push(MetadataFinding::from_record(&record, "unsafe_storage_key")),
        }
    }
    let physical = physical_keys.into_iter().collect::<BTreeSet<_>>();
    inspect_metadata(storage, &metadata, &physical, &mut report)?;
    inspect_physical(&metadata, &physical, &mut report);
    report.finish();
    Ok(report)
}

fn inspect_metadata(
    storage: &dyn ObjectStorage,
    metadata: &BTreeMap<StorageKey, blobyard_contract::ObjectVersionRecord>,
    physical: &BTreeSet<StorageKey>,
    report: &mut ReconciliationReport,
) -> Result<(), ServerError> {
    for (key, record) in metadata {
        if record.state != UploadState::Complete {
            if physical.contains(key) {
                report.orphaned_objects.push(MetadataFinding::from_record(
                    record,
                    "non_complete_metadata",
                ));
            }
            continue;
        }
        let Some(expected) = expected_metadata(record, report) else {
            continue;
        };
        if !physical.contains(key) {
            report.missing_bytes.push(MetadataFinding::from_record(
                record,
                "physical_object_absent",
            ));
            continue;
        }
        inspect_complete(storage, key, record, &expected, report)?;
    }
    Ok(())
}

fn inspect_complete(
    storage: &dyn ObjectStorage,
    key: &StorageKey,
    record: &blobyard_contract::ObjectVersionRecord,
    expected: &ObservedObject,
    report: &mut ReconciliationReport,
) -> Result<(), ServerError> {
    match observe(storage, key) {
        Ok(actual) if actual == *expected => Ok(()),
        Ok(actual) => {
            report.integrity_disagreements.push(IntegrityFinding::new(
                record,
                expected,
                Some(&actual),
                "content_mismatch",
            ));
            Ok(())
        }
        Err(StorageError::NotFound) => {
            report.missing_bytes.push(MetadataFinding::from_record(
                record,
                "physical_object_absent",
            ));
            Ok(())
        }
        Err(StorageError::IntegrityMismatch) => {
            report.integrity_disagreements.push(IntegrityFinding::new(
                record,
                expected,
                None,
                "storage_integrity_unreadable",
            ));
            Ok(())
        }
        Err(error) => Err(storage_error(error)),
    }
}

fn expected_metadata(
    record: &blobyard_contract::ObjectVersionRecord,
    report: &mut ReconciliationReport,
) -> Option<ObservedObject> {
    let Some(size) = record.size else {
        report.invalid_metadata.push(MetadataFinding::from_record(
            record,
            "missing_integrity_metadata",
        ));
        return None;
    };
    let checksum = record
        .checksum
        .as_ref()
        .and_then(|value| ObjectChecksum::new(value.clone()).ok());
    if let Some(checksum) = checksum {
        Some(ObservedObject { size, checksum })
    } else {
        report.invalid_metadata.push(MetadataFinding::from_record(
            record,
            "missing_integrity_metadata",
        ));
        None
    }
}

fn observe(storage: &dyn ObjectStorage, key: &StorageKey) -> Result<ObservedObject, StorageError> {
    let mut read = storage.get(key, None)?;
    let mut digest = Sha256::new();
    let size = hash_reader(&mut read.reader, &mut digest)?;
    let actual = ObservedObject {
        size,
        checksum: ObjectChecksum::from_sha256_digest(digest.finalize().into()),
    };
    if actual.size != read.metadata.size || actual.checksum != read.metadata.checksum {
        Err(StorageError::IntegrityMismatch)
    } else {
        Ok(actual)
    }
}

fn hash_reader(reader: &mut dyn Read, digest: &mut Sha256) -> Result<u64, StorageError> {
    hash_reader_from(reader, digest, 0)
}

fn hash_reader_from(
    reader: &mut dyn Read,
    digest: &mut Sha256,
    mut size: u64,
) -> Result<u64, StorageError> {
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = reader
            .read(buffer.as_mut())
            .map_err(|_error| StorageError::Unavailable)?;
        if count == 0 {
            return Ok(size);
        }
        digest.update(&buffer[..count]);
        size = add_size(size, count as u64)?;
    }
}

fn add_size(current: u64, count: u64) -> Result<u64, StorageError> {
    current
        .checked_add(count)
        .ok_or(StorageError::IntegrityMismatch)
}

trait ReconciliationRepository: Send + Sync {
    fn schema_version(&self) -> Result<u32, RepositoryError>;
    fn list_object_versions(&self) -> Result<Vec<ObjectVersionRecord>, RepositoryError>;
}

impl<T> ReconciliationRepository for T
where
    T: MetadataRepository + MetadataRepositoryInventory,
{
    fn schema_version(&self) -> Result<u32, RepositoryError> {
        MetadataRepository::schema_version(self)
    }

    fn list_object_versions(&self) -> Result<Vec<ObjectVersionRecord>, RepositoryError> {
        MetadataRepositoryInventory::list_object_versions(self)
    }
}

fn inspect_physical(
    metadata: &BTreeMap<StorageKey, blobyard_contract::ObjectVersionRecord>,
    physical: &BTreeSet<StorageKey>,
    report: &mut ReconciliationReport,
) {
    for key in physical {
        if !metadata.contains_key(key) {
            report.missing_metadata.push(PhysicalFinding {
                storage_key: key.as_str().to_owned(),
            });
        }
    }
}

fn encode(report: &ReconciliationReport) -> Result<String, ServerError> {
    serde_json::to_string_pretty(&report).map_err(|_error| ServerError::Initialization)
}

const fn storage_error(_error: StorageError) -> ServerError {
    ServerError::Storage
}

#[cfg(test)]
#[path = "reconciliation_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reconciliation_failure_tests.rs"]
mod failure_tests;
