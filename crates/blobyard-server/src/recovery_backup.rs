use super::{RecoveryError, io, manifest::BackupManifest, manifest::BackupObject, map_repository};
use crate::StorageConfiguration;
use blobyard_contract::{
    MetadataRepositoryInventory, ObjectChecksum, ObjectStorage, ObjectVersionRecord, UploadState,
};
use blobyard_repository_sqlite::{
    DatabaseInspection, SqliteRepository, current_schema_version, snapshot_database,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupReport {
    report_schema_version: u32,
    operation: &'static str,
    core_version: &'static str,
    metadata_schema_version: u32,
    objects: usize,
    bytes: u64,
    destination_ready: bool,
}

pub(super) fn create(
    data_directory: &Path,
    output: &Path,
    storage_configuration: &StorageConfiguration,
) -> Result<BackupReport, RecoveryError> {
    io::validate_directory(data_directory)?;
    let stage = io::create_stage(output)?;
    let (metadata_path, inspection) = stage_control_files(data_directory, stage.path())?;
    let records = inventory_records(&metadata_path)?;
    let (objects, bytes) =
        stage_objects(data_directory, stage.path(), storage_configuration, records)?;
    let manifest = write_manifest(stage.path(), &metadata_path, inspection, objects)?;
    let report = BackupReport {
        report_schema_version: 1,
        operation: "backup",
        core_version: env!("CARGO_PKG_VERSION"),
        metadata_schema_version: inspection.schema_version,
        objects: manifest.objects.len(),
        bytes,
        destination_ready: true,
    };
    #[cfg(test)]
    apply_fault(BackupFault::BlockPersistence, || {
        std::fs::create_dir(output).and_then(|()| std::fs::write(output.join("occupied"), b"x"))
    });
    io::persist_stage(stage, output)?;
    Ok(report)
}

fn stage_control_files(
    data_directory: &Path,
    stage: &Path,
) -> Result<(PathBuf, DatabaseInspection), RecoveryError> {
    let metadata_path = stage.join("metadata.sqlite3");
    let inspection = snapshot_database(&data_directory.join("metadata.sqlite3"), &metadata_path)
        .map_err(map_repository)?;
    #[cfg(test)]
    apply_fault(BackupFault::RemoveSnapshot, || {
        std::fs::remove_file(&metadata_path)
    });
    io::set_private_file(&metadata_path)?;
    if inspection.schema_version != current_schema_version() {
        return Err(RecoveryError::SchemaTooOld);
    }
    let runtime_secret = read_runtime_secret(data_directory)?;
    #[cfg(test)]
    apply_fault(BackupFault::BlockRuntimeSecret, || {
        std::fs::write(stage.join("runtime.secret"), b"blocked")
    });
    io::write_private_file(&stage.join("runtime.secret"), &runtime_secret)?;
    Ok((metadata_path, inspection))
}

fn inventory_records(metadata_path: &Path) -> Result<Vec<ObjectVersionRecord>, RecoveryError> {
    #[cfg(test)]
    apply_fault(BackupFault::CorruptSnapshot, || {
        std::fs::write(metadata_path, b"not sqlite")
    });
    let repository = SqliteRepository::open(metadata_path).map_err(map_repository)?;
    #[cfg(test)]
    apply_fault(BackupFault::DropInventoryTable, || {
        rusqlite::Connection::open(metadata_path).and_then(|connection| {
            connection
                .execute("DROP TABLE object_versions", [])
                .map(|_changed| ())
        })
    });
    let records = repository.list_object_versions().map_err(map_repository)?;
    if records
        .iter()
        .any(|record| record.state == UploadState::Pending)
    {
        return Err(RecoveryError::ActiveUploads);
    }
    Ok(records)
}

fn stage_objects(
    data_directory: &Path,
    stage: &Path,
    storage_configuration: &StorageConfiguration,
    records: Vec<ObjectVersionRecord>,
) -> Result<(Vec<BackupObject>, u64), RecoveryError> {
    let storage = storage_configuration
        .open(data_directory)
        .map_err(|_error| RecoveryError::Storage)?;
    #[cfg(test)]
    apply_fault(BackupFault::RemoveStoredObject, || {
        std::fs::remove_file(
            data_directory
                .join("objects/objects")
                .join("objects/version_recovery"),
        )
    });
    let mut objects = Vec::new();
    let mut bytes = 0_u64;
    for record in records
        .into_iter()
        .filter(|record| record.state == UploadState::Complete)
    {
        let object = copy_object(stage, storage.as_ref(), &record)?;
        #[cfg(test)]
        if fault_is(BackupFault::OverflowByteTotal) {
            bytes = u64::MAX;
        }
        bytes = add_bytes(bytes, object.size)?;
        objects.push(object);
    }
    Ok((objects, bytes))
}

fn write_manifest(
    stage: &Path,
    metadata_path: &Path,
    inspection: DatabaseInspection,
    objects: Vec<BackupObject>,
) -> Result<BackupManifest, RecoveryError> {
    #[cfg(test)]
    apply_fault(BackupFault::RemoveMetadataBeforeHash, || {
        std::fs::remove_file(metadata_path)
    });
    #[cfg(test)]
    apply_fault(BackupFault::RemoveSecretBeforeHash, || {
        std::fs::remove_file(stage.join("runtime.secret"))
    });
    let manifest = BackupManifest::new(
        inspection.schema_version,
        io::hash_file(metadata_path)?,
        io::hash_file(&stage.join("runtime.secret"))?,
        objects,
    );
    #[cfg(test)]
    apply_fault(BackupFault::BlockManifest, || {
        std::fs::write(stage.join("manifest.json"), b"blocked")
    });
    manifest.write(stage)?;
    Ok(manifest)
}

fn copy_object(
    root: &Path,
    storage: &dyn ObjectStorage,
    record: &blobyard_contract::ObjectVersionRecord,
) -> Result<BackupObject, RecoveryError> {
    let key = blobyard_contract::StorageKey::new(record.storage_key.clone())
        .map_err(|_error| RecoveryError::Integrity)?;
    let size = record.size.ok_or(RecoveryError::Integrity)?;
    let checksum = record
        .checksum
        .as_ref()
        .ok_or(RecoveryError::Integrity)
        .and_then(|value| {
            ObjectChecksum::new(value.clone()).map_err(|_error| RecoveryError::Integrity)
        })?;
    let mut read = storage.get(&key, None).map_err(map_storage)?;
    if read.metadata.size != size || read.metadata.checksum != checksum {
        return Err(RecoveryError::Integrity);
    }
    let target = root.join("objects").join(key.as_str());
    let (actual_size, actual_checksum) = io::copy_verified(&mut read.reader, &target)?;
    if actual_size != size || actual_checksum != checksum.as_str() {
        return Err(RecoveryError::Integrity);
    }
    Ok(BackupObject::new(
        record.storage_key.clone(),
        size,
        checksum.as_str().to_owned(),
    ))
}

fn read_runtime_secret(data_directory: &Path) -> Result<Vec<u8>, RecoveryError> {
    let bytes = io::read_secure_file(data_directory, Path::new("runtime.secret"))
        .map_err(|_error| RecoveryError::InstallationUnavailable)?;
    let value =
        std::str::from_utf8(&bytes).map_err(|_error| RecoveryError::InstallationUnavailable)?;
    blobyard_core::SecretString::new(value.to_owned())
        .map_err(|_error| RecoveryError::InstallationUnavailable)?;
    Ok(bytes)
}

const fn map_storage(error: blobyard_contract::StorageError) -> RecoveryError {
    match error {
        blobyard_contract::StorageError::IntegrityMismatch => RecoveryError::Integrity,
        blobyard_contract::StorageError::NotFound
        | blobyard_contract::StorageError::Conflict
        | blobyard_contract::StorageError::InvalidInput
        | blobyard_contract::StorageError::Unavailable => RecoveryError::Storage,
    }
}

fn add_bytes(total: u64, object_size: u64) -> Result<u64, RecoveryError> {
    total
        .checked_add(object_size)
        .ok_or(RecoveryError::Integrity)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackupFault {
    RemoveSnapshot,
    BlockRuntimeSecret,
    CorruptSnapshot,
    DropInventoryTable,
    RemoveStoredObject,
    OverflowByteTotal,
    RemoveMetadataBeforeHash,
    RemoveSecretBeforeHash,
    BlockManifest,
    BlockPersistence,
}

#[cfg(test)]
thread_local! {
    static BACKUP_FAULT: std::cell::Cell<Option<BackupFault>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn set_fault(fault: BackupFault) {
    BACKUP_FAULT.with(|slot| slot.set(Some(fault)));
}

#[cfg(test)]
fn fault_is(fault: BackupFault) -> bool {
    BACKUP_FAULT.with(|slot| slot.get() == Some(fault))
}

#[cfg(test)]
fn apply_fault<E>(fault: BackupFault, action: impl FnOnce() -> Result<(), E>) {
    if fault_is(fault) {
        let _ignored = action();
    }
}

#[cfg(test)]
#[path = "recovery_backup_tests.rs"]
mod tests;
