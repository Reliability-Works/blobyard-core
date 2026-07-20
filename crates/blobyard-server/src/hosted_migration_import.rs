use super::{
    HostedMigrationError, HostedMigrationOptions, destination_parent, export::DownloadedObjects,
    projection::PreparedMigration,
};
use crate::RuntimeStorage;
use blobyard_contract::{
    CredentialRepository, MigrationRepository, ObjectChecksum, ObjectStorage,
    ObjectStorageInventory, StorageError, StorageKey,
};
use blobyard_core::SecretString;
use blobyard_repository_sqlite::SqliteRepository;
use std::path::Path;

pub(super) fn activate(
    options: &HostedMigrationOptions,
    prepared: &PreparedMigration,
    downloaded: &DownloadedObjects,
    bootstrap: &SecretString,
) -> Result<(), HostedMigrationError> {
    let parent = destination_parent(&options.data_directory)?;
    reject_existing(&options.data_directory)?;
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::BlockParentDirectory) {
        let _ignored = std::fs::write(parent, b"blocked");
    }
    std::fs::create_dir_all(parent).map_err(|_error| HostedMigrationError::Persistence)?;
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::BlockTemporaryDirectory) {
        let _ignored = std::fs::remove_dir(parent);
        let _ignored = std::fs::write(parent, b"blocked");
    }
    let temporary = tempfile::Builder::new()
        .prefix(".blobyard-hosted-migration-")
        .tempdir_in(parent)
        .map_err(|_error| HostedMigrationError::Persistence)?;
    let installation = temporary.path().join("installation");
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::BlockInstallationDirectory) {
        let _ignored = std::fs::write(&installation, b"blocked");
    }
    std::fs::create_dir(&installation).map_err(|_error| HostedMigrationError::Persistence)?;
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::BlockMetadata) {
        let _ignored = std::fs::create_dir(installation.join("metadata.sqlite3"));
    }
    let repository = SqliteRepository::open(&installation.join("metadata.sqlite3"))
        .map_err(|_error| HostedMigrationError::Metadata)?;
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::BlockStorage) {
        let _ignored = std::fs::write(installation.join("objects"), b"blocked");
    }
    let storage = options
        .storage
        .open(&installation)
        .map_err(|_error| HostedMigrationError::Storage)?;
    require_empty_storage(storage.as_ref())?;
    let imported = import_objects(storage.as_ref(), prepared, downloaded)?;
    if let Err(error) = activate_metadata(&repository, &installation, prepared, bootstrap) {
        cleanup(storage.as_ref(), &imported)?;
        return Err(error);
    }
    drop(repository);
    drop(storage);
    let rename_result = rename_installation(&installation, &options.data_directory);
    if rename_result.is_err() {
        #[cfg(any(test, feature = "test-seams"))]
        if fault_is(ActivationFault::ReopenStorage) {
            let objects = installation.join("objects");
            let _ignored = std::fs::remove_dir_all(&objects);
            let _ignored = std::fs::write(objects, b"blocked");
        }
        let storage = options
            .storage
            .open(&installation)
            .map_err(|_error| HostedMigrationError::Persistence)?;
        cleanup(storage.as_ref(), &imported)?;
        return Err(HostedMigrationError::Persistence);
    }
    sync_parent(parent)
}

fn rename_installation(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::Rename)
        || fault_is(ActivationFault::ReopenStorage)
        || fault_is(ActivationFault::RenameCleanup)
    {
        return Err(std::io::Error::other("fixture rename failure"));
    }
    std::fs::rename(from, to)
}

fn reject_existing(destination: &Path) -> Result<(), HostedMigrationError> {
    match destination.try_exists() {
        Ok(false) => Ok(()),
        Ok(true) => Err(HostedMigrationError::DestinationExists),
        Err(_error) => Err(HostedMigrationError::Persistence),
    }
}

fn require_empty_storage(storage: &dyn RuntimeStorage) -> Result<(), HostedMigrationError> {
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::StorageNotEmpty) {
        return Err(HostedMigrationError::StorageNotEmpty);
    }
    let keys = ObjectStorageInventory::list_object_keys(storage)
        .map_err(|_error| HostedMigrationError::Storage)?;
    if keys.is_empty() {
        Ok(())
    } else {
        Err(HostedMigrationError::StorageNotEmpty)
    }
}

fn import_objects(
    storage: &dyn RuntimeStorage,
    prepared: &PreparedMigration,
    downloaded: &DownloadedObjects,
) -> Result<Vec<StorageKey>, HostedMigrationError> {
    let mut imported = Vec::new();
    for object in &prepared.snapshot.objects {
        let path = downloaded
            .paths
            .get(&object.id)
            .ok_or(HostedMigrationError::Integrity)?;
        let mut source =
            std::fs::File::open(path).map_err(|_error| HostedMigrationError::Persistence)?;
        let key = StorageKey::new(object.storage_key.clone())
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        let checksum = ObjectChecksum::new(object.checksum.clone())
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        match ObjectStorage::put(storage, &key, &mut source, Some(&checksum)) {
            Ok(metadata) if metadata.size == object.size && metadata.checksum == checksum => {
                imported.push(key);
            }
            Ok(_metadata) => {
                imported.push(key);
                cleanup(storage, &imported)?;
                return Err(HostedMigrationError::Integrity);
            }
            Err(StorageError::IntegrityMismatch) => {
                cleanup(storage, &imported)?;
                return Err(HostedMigrationError::Integrity);
            }
            Err(_error) => {
                cleanup(storage, &imported)?;
                return Err(HostedMigrationError::Storage);
            }
        }
    }
    Ok(imported)
}

fn activate_metadata(
    repository: &SqliteRepository,
    installation: &Path,
    prepared: &PreparedMigration,
    bootstrap: &SecretString,
) -> Result<(), HostedMigrationError> {
    repository
        .import_migration(&prepared.snapshot)
        .map_err(|_error| HostedMigrationError::Metadata)?;
    crate::runtime_secret(installation).map_err(|_error| HostedMigrationError::Persistence)?;
    let installed = repository
        .install_bootstrap(&crate::auth::hash(bootstrap.expose_secret()))
        .map_err(|_error| HostedMigrationError::Metadata)?;
    if installed {
        Ok(())
    } else {
        Err(HostedMigrationError::Metadata)
    }
}

fn cleanup(
    storage: &dyn RuntimeStorage,
    imported: &[StorageKey],
) -> Result<(), HostedMigrationError> {
    #[cfg(any(test, feature = "test-seams"))]
    if fault_is(ActivationFault::Cleanup) || fault_is(ActivationFault::RenameCleanup) {
        return Err(HostedMigrationError::Persistence);
    }
    let mut failed = false;
    for key in imported.iter().rev() {
        if ObjectStorage::delete(storage, key).is_err() {
            failed = true;
        }
    }
    if failed {
        Err(HostedMigrationError::Persistence)
    } else {
        Ok(())
    }
}

fn sync_parent(parent: &Path) -> Result<(), HostedMigrationError> {
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_error| HostedMigrationError::Persistence)
}

#[cfg(any(test, feature = "test-seams"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivationFault {
    BlockParentDirectory,
    BlockTemporaryDirectory,
    BlockInstallationDirectory,
    BlockMetadata,
    BlockStorage,
    StorageNotEmpty,
    Cleanup,
    Rename,
    ReopenStorage,
    RenameCleanup,
}

#[cfg(any(test, feature = "test-seams"))]
thread_local! {
    static ACTIVATION_FAULT: std::cell::Cell<Option<ActivationFault>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(any(test, feature = "test-seams"))]
fn fault_is(fault: ActivationFault) -> bool {
    ACTIVATION_FAULT.with(|slot| slot.get() == Some(fault))
}

#[cfg(test)]
fn with_fault<T>(fault: ActivationFault, action: impl FnOnce() -> T) -> T {
    ACTIVATION_FAULT.with(|slot| slot.set(Some(fault)));
    let result = action();
    ACTIVATION_FAULT.with(|slot| slot.set(None));
    result
}

#[cfg(test)]
#[path = "hosted_migration_import_test_storage.rs"]
mod test_storage;

#[cfg(test)]
#[path = "hosted_migration_import_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hosted_migration_import_edge_tests.rs"]
mod edge_tests;
