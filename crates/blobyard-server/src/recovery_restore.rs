use super::{RecoveryError, io, manifest::BackupManifest};
use crate::{RuntimeStorage, StorageConfiguration};
use blobyard_contract::{ObjectChecksum, ObjectStorage, StorageKey};
use blobyard_repository_sqlite::{
    current_schema_version, inspect_database, oldest_supported_schema_version,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RestoreReport {
    report_schema_version: u32,
    operation: &'static str,
    core_version: &'static str,
    metadata_schema_version: u32,
    objects: usize,
    bytes: u64,
    installation_ready: bool,
}

pub(super) fn apply(
    backup: &Path,
    data_directory: &Path,
    storage_configuration: &StorageConfiguration,
) -> Result<RestoreReport, RecoveryError> {
    let manifest = validate_backup(backup)?;
    let bytes = total_bytes(&manifest)?;
    let stage = io::create_stage(data_directory)?;
    #[cfg(test)]
    apply_fault(RestoreFault::RemoveControlFile, || {
        std::fs::remove_file(backup.join("runtime.secret"))
    });
    copy_control_files(backup, stage.path(), &manifest)?;
    let storage = storage_configuration
        .open(stage.path())
        .map_err(|_error| RecoveryError::Storage)?;
    #[cfg(test)]
    if fault_is(RestoreFault::SeedStorage) || fault_is(RestoreFault::SeedStorageError) {
        let key = if fault_is(RestoreFault::SeedStorageError) {
            "../invalid"
        } else {
            "objects/recovery-fault"
        };
        seed_storage(storage.as_ref(), key)?;
    }
    require_empty_storage(storage.as_ref())?;
    #[cfg(test)]
    apply_fault(RestoreFault::CorruptObject, || {
        std::fs::write(
            backup.join("objects").join("objects/version_recovery"),
            b"tampered",
        )
    });
    let mut imported = Vec::new();
    let restored = restore_objects(backup, storage.as_ref(), &manifest, &mut imported);
    #[cfg(test)]
    apply_fault(RestoreFault::BlockPersistence, || {
        std::fs::create_dir(data_directory)
            .and_then(|()| std::fs::write(data_directory.join("occupied"), b"x"))
    });
    persist_restored_stage(restored, stage, data_directory, storage.as_ref(), &imported)?;
    Ok(RestoreReport {
        report_schema_version: 1,
        operation: "restore",
        core_version: env!("CARGO_PKG_VERSION"),
        metadata_schema_version: manifest.metadata_schema_version,
        objects: manifest.objects.len(),
        bytes,
        installation_ready: true,
    })
}

fn require_empty_storage(storage: &dyn RuntimeStorage) -> Result<(), RecoveryError> {
    if storage
        .list_object_keys()
        .map_err(|_error| RecoveryError::Storage)?
        .is_empty()
    {
        Ok(())
    } else {
        Err(RecoveryError::StorageNotEmpty)
    }
}

fn persist_restored_stage(
    restored: Result<(), RecoveryError>,
    stage: tempfile::TempDir,
    data_directory: &Path,
    storage: &dyn ObjectStorage,
    imported: &[StorageKey],
) -> Result<(), RecoveryError> {
    if let Err(error) = restored {
        return cleanup_after(storage, imported, error);
    }
    io::persist_stage_with_cleanup(stage, data_directory, || cleanup(storage, imported))
}

fn cleanup_after(
    storage: &dyn ObjectStorage,
    imported: &[StorageKey],
    error: RecoveryError,
) -> Result<(), RecoveryError> {
    cleanup(storage, imported).and(Err(error))
}

fn validate_backup(backup: &Path) -> Result<BackupManifest, RecoveryError> {
    let manifest = BackupManifest::read(backup)?;
    let schema = manifest.metadata_schema_version;
    if schema < oldest_supported_schema_version() {
        return Err(RecoveryError::SchemaTooOld);
    }
    if schema > current_schema_version() {
        return Err(RecoveryError::SchemaTooNew);
    }
    verify_hash(
        backup,
        Path::new("metadata.sqlite3"),
        &manifest.metadata_sha256,
    )?;
    verify_hash(
        backup,
        Path::new("runtime.secret"),
        &manifest.runtime_secret_sha256,
    )?;
    #[cfg(test)]
    apply_fault(RestoreFault::RemoveMetadataAfterHashes, || {
        std::fs::remove_file(backup.join("metadata.sqlite3"))
    });
    drop(io::open_secure_file(backup, Path::new("metadata.sqlite3"))?);
    let inspection = inspect_database(&backup.join("metadata.sqlite3"))
        .map_err(|_error| RecoveryError::Database)?;
    if inspection.schema_version != schema {
        return Err(RecoveryError::InvalidBackup);
    }
    #[cfg(test)]
    apply_fault(RestoreFault::RemoveSecretAfterHashes, || {
        std::fs::remove_file(backup.join("runtime.secret"))
    });
    let runtime = io::read_secure_file(backup, Path::new("runtime.secret"))?;
    let value = std::str::from_utf8(&runtime).map_err(|_error| RecoveryError::InvalidBackup)?;
    blobyard_core::SecretString::new(value.to_owned())
        .map_err(|_error| RecoveryError::InvalidBackup)?;
    Ok(manifest)
}

fn copy_control_files(
    backup: &Path,
    stage: &Path,
    manifest: &BackupManifest,
) -> Result<(), RecoveryError> {
    for (name, checksum) in [
        ("metadata.sqlite3", manifest.metadata_sha256.as_str()),
        ("runtime.secret", manifest.runtime_secret_sha256.as_str()),
    ] {
        let mut source = io::open_secure_file(backup, Path::new(name))?;
        let (size, actual) = io::copy_verified(&mut source, &stage.join(name))?;
        if size == 0 || actual != checksum {
            return Err(RecoveryError::Integrity);
        }
    }
    Ok(())
}

fn restore_objects(
    backup: &Path,
    storage: &dyn RuntimeStorage,
    manifest: &BackupManifest,
    imported: &mut Vec<StorageKey>,
) -> Result<(), RecoveryError> {
    for object in &manifest.objects {
        let key = StorageKey::new(object.storage_key.clone())
            .map_err(|_error| RecoveryError::InvalidBackup)?;
        let expected = ObjectChecksum::new(object.checksum.clone())
            .map_err(|_error| RecoveryError::InvalidBackup)?;
        let relative = Path::new("objects").join(key.as_str());
        let mut validation = io::open_secure_file(backup, &relative)?;
        let (size, checksum) = io::hash_reader(&mut validation)?;
        if size != object.size || checksum != object.checksum {
            return Err(RecoveryError::Integrity);
        }
        #[cfg(test)]
        apply_fault(RestoreFault::RemoveObjectAfterHash, || {
            std::fs::remove_file(backup.join(&relative))
        });
        let mut source = io::open_secure_file(backup, &relative)?;
        let metadata = storage
            .put(&key, &mut source, Some(&expected))
            .map_err(|_error| RecoveryError::Storage)?;
        imported.push(key.clone());
        if metadata.size != object.size || metadata.checksum != expected {
            return Err(RecoveryError::Integrity);
        }
    }
    let physical = storage
        .list_object_keys()
        .map_err(|_error| RecoveryError::Storage)?;
    if physical == *imported {
        Ok(())
    } else {
        Err(RecoveryError::Integrity)
    }
}

fn verify_hash(root: &Path, relative: &Path, expected: &str) -> Result<(), RecoveryError> {
    let mut source = io::open_secure_file(root, relative)?;
    let (_size, actual) = io::hash_reader(&mut source)?;
    if actual == expected {
        Ok(())
    } else {
        Err(RecoveryError::Integrity)
    }
}

fn total_bytes(manifest: &BackupManifest) -> Result<u64, RecoveryError> {
    manifest.objects.iter().try_fold(0_u64, |total, object| {
        total
            .checked_add(object.size)
            .ok_or(RecoveryError::Integrity)
    })
}

fn cleanup(storage: &dyn ObjectStorage, keys: &[StorageKey]) -> Result<(), RecoveryError> {
    let mut failed = false;
    for key in keys.iter().rev() {
        failed |= storage.delete(key).is_err();
    }
    if failed {
        Err(RecoveryError::Storage)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn seed_storage(storage: &dyn ObjectStorage, key: &str) -> Result<(), RecoveryError> {
    let key = StorageKey::new(key.to_owned()).map_err(|_error| RecoveryError::Storage)?;
    storage
        .put(&key, &mut std::io::Cursor::new(b"occupied"), None)
        .map_err(|_error| RecoveryError::Storage)?;
    Ok(())
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestoreFault {
    RemoveControlFile,
    SeedStorage,
    SeedStorageError,
    CorruptObject,
    BlockPersistence,
    RemoveMetadataAfterHashes,
    RemoveSecretAfterHashes,
    RemoveObjectAfterHash,
}

#[cfg(test)]
thread_local! {
    static RESTORE_FAULT: std::cell::Cell<Option<RestoreFault>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn set_fault(fault: RestoreFault) {
    RESTORE_FAULT.with(|slot| slot.set(Some(fault)));
}

#[cfg(test)]
fn fault_is(fault: RestoreFault) -> bool {
    RESTORE_FAULT.with(|slot| slot.get() == Some(fault))
}

#[cfg(test)]
fn apply_fault<E>(fault: RestoreFault, action: impl FnOnce() -> Result<(), E>) {
    if fault_is(fault) {
        let _ignored = action();
    }
}

#[cfg(test)]
#[path = "recovery_restore_tests.rs"]
mod tests;
