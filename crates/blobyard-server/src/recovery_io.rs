use super::RecoveryError;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

pub(super) fn validate_directory(path: &Path) -> Result<(), RecoveryError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_error| RecoveryError::InstallationUnavailable)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(RecoveryError::InstallationUnavailable)
    }
}

pub(super) fn read_secure_file(root: &Path, relative: &Path) -> Result<Vec<u8>, RecoveryError> {
    let mut file = open_secure_file(root, relative)?;
    read_all(&mut file)
}

fn read_all(source: &mut dyn Read) -> Result<Vec<u8>, RecoveryError> {
    let mut bytes = Vec::new();
    source
        .read_to_end(&mut bytes)
        .map_err(|_error| RecoveryError::InvalidBackup)?;
    Ok(bytes)
}

pub(super) fn open_secure_file(root: &Path, relative: &Path) -> Result<File, RecoveryError> {
    let path = secure_file_path(root, relative)?;
    File::open(path).map_err(|_error| RecoveryError::InvalidBackup)
}

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), RecoveryError> {
    create_private_parents(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_error| RecoveryError::Persistence)?;
    set_private_file(path)?;
    file.write_all(bytes)
        .and_then(|()| sync_file(&file))
        .map_err(|_error| RecoveryError::Persistence)
}

pub(super) fn copy_verified(
    source: &mut dyn Read,
    target: &Path,
) -> Result<(u64, String), RecoveryError> {
    create_private_parents(target)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|_error| RecoveryError::Persistence)?;
    set_private_file(target)?;
    let result = stream_hash(source, &mut file)?;
    sync_file(&file).map_err(|_error| RecoveryError::Persistence)?;
    Ok(result)
}

pub(super) fn hash_file(path: &Path) -> Result<String, RecoveryError> {
    let mut file = File::open(path).map_err(|_error| RecoveryError::InvalidBackup)?;
    hash_reader(&mut file).map(|(_size, checksum)| checksum)
}

pub(super) fn hash_reader(source: &mut dyn Read) -> Result<(u64, String), RecoveryError> {
    #[cfg(test)]
    if fault_is(IoFault::HashRead) {
        return Err(RecoveryError::Storage);
    }
    stream_hash(source, &mut std::io::sink())
}

pub(super) fn set_private_file(path: &Path) -> Result<(), RecoveryError> {
    #[cfg(test)]
    if fault_is(IoFault::Permission) {
        return Err(RecoveryError::Persistence);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_error| RecoveryError::Persistence)
}

fn sync_file(file: &File) -> std::io::Result<()> {
    #[cfg(test)]
    if fault_is(IoFault::Sync) {
        return Err(std::io::Error::other("fixture sync failure"));
    }
    file.sync_all()
}

pub(super) fn persist_stage(
    stage: tempfile::TempDir,
    destination: &Path,
) -> Result<(), RecoveryError> {
    persist_stage_with_cleanup(stage, destination, || Ok(()))
}

pub(super) fn persist_stage_with_cleanup(
    stage: tempfile::TempDir,
    destination: &Path,
    cleanup: impl FnOnce() -> Result<(), RecoveryError>,
) -> Result<(), RecoveryError> {
    let path = stage.keep();
    match fs::rename(&path, destination) {
        Ok(()) => Ok(()),
        Err(_error) => {
            let cleanup_result = cleanup();
            let _ignored = fs::remove_dir_all(path);
            cleanup_result.and(Err(RecoveryError::Persistence))
        }
    }
}

pub(super) fn create_stage(destination: &Path) -> Result<tempfile::TempDir, RecoveryError> {
    if destination.exists() {
        return Err(RecoveryError::DestinationExists);
    }
    let parent = usable_parent(destination)?;
    fs::create_dir_all(&parent).map_err(|_error| RecoveryError::Persistence)?;
    #[cfg(test)]
    if fault_is(IoFault::BlockTempDirectory) {
        let _ignored = fs::remove_dir(&parent);
        let _ignored = fs::write(&parent, b"blocked");
    }
    let stage = tempfile::Builder::new()
        .prefix(".blobyard-recovery-")
        .tempdir_in(parent)
        .map_err(|_error| RecoveryError::Persistence)?;
    #[cfg(test)]
    if fault_is(IoFault::RemoveStage) {
        let _ignored = fs::remove_dir(stage.path());
    }
    fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o700))
        .map_err(|_error| RecoveryError::Persistence)?;
    Ok(stage)
}

fn usable_parent(path: &Path) -> Result<PathBuf, RecoveryError> {
    if path.file_name().is_none() {
        return Err(RecoveryError::Persistence);
    }
    let parent = path.with_file_name("");
    if parent.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(parent)
    }
}

fn secure_file_path(root: &Path, relative: &Path) -> Result<PathBuf, RecoveryError> {
    validate_directory(root).map_err(|_error| RecoveryError::InvalidBackup)?;
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RecoveryError::InvalidBackup);
    }
    let mut path = root.to_owned();
    for component in relative.components() {
        path.push(component.as_os_str());
        let metadata =
            fs::symlink_metadata(&path).map_err(|_error| RecoveryError::InvalidBackup)?;
        if metadata.file_type().is_symlink() {
            return Err(RecoveryError::InvalidBackup);
        }
    }
    #[cfg(test)]
    if fault_is(IoFault::RemoveSecureTarget) {
        let _ignored = fs::remove_file(&path);
    }
    let metadata = fs::metadata(&path).map_err(|_error| RecoveryError::InvalidBackup)?;
    if metadata.is_file() {
        Ok(path)
    } else {
        Err(RecoveryError::InvalidBackup)
    }
}

fn create_private_parents(path: &Path) -> Result<(), RecoveryError> {
    let parent = path.parent().ok_or(RecoveryError::Persistence)?;
    fs::create_dir_all(parent).map_err(|_error| RecoveryError::Persistence)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|_error| RecoveryError::Persistence)
}

fn stream_hash(
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<(u64, String), RecoveryError> {
    stream_hash_from(source, target, 0)
}

fn stream_hash_from(
    source: &mut dyn Read,
    target: &mut dyn Write,
    initial_size: u64,
) -> Result<(u64, String), RecoveryError> {
    let mut digest = Sha256::new();
    let mut size = initial_size;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = source
            .read(buffer.as_mut())
            .map_err(|_error| RecoveryError::Storage)?;
        if count == 0 {
            return Ok((size, blobyard_core::hex_digest(&digest.finalize())));
        }
        target
            .write_all(&buffer[..count])
            .map_err(|_error| RecoveryError::Persistence)?;
        digest.update(&buffer[..count]);
        size = add_size(size, count as u64)?;
    }
}

fn add_size(total: u64, count: u64) -> Result<u64, RecoveryError> {
    total.checked_add(count).ok_or(RecoveryError::Integrity)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IoFault {
    Permission,
    Sync,
    BlockTempDirectory,
    RemoveStage,
    RemoveSecureTarget,
    HashRead,
}

#[cfg(test)]
thread_local! {
    static IO_FAULT: std::cell::Cell<Option<IoFault>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn fault_is(fault: IoFault) -> bool {
    IO_FAULT.with(|slot| slot.get() == Some(fault))
}

#[cfg(test)]
pub(super) fn with_fault<T>(fault: IoFault, action: impl FnOnce() -> T) -> T {
    IO_FAULT.with(|slot| slot.set(Some(fault)));
    let result = action();
    IO_FAULT.with(|slot| slot.set(None));
    result
}

#[cfg(test)]
#[path = "recovery_io_tests.rs"]
mod tests;
