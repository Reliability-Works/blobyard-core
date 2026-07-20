#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{StoragePaths, reject_symlink, secure_parent};
use blobyard_contract::{MultipartId, StorageError};
use std::path::Path;

#[test]
fn path_guards_reject_parentless_targets() {
    assert_eq!(
        secure_parent(Path::new("/")),
        Err(StorageError::InvalidInput)
    );
}

#[test]
fn path_creation_maps_root_and_internal_directory_failures() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root_blocker = temporary.path().join("root-blocker");
    std::fs::write(&root_blocker, b"file").expect("root blocker");
    assert!(matches!(
        StoragePaths::create(&root_blocker.join("storage")),
        Err(StorageError::Unavailable)
    ));

    let internal_blocker = temporary.path().join("internal-blocker");
    std::fs::create_dir(&internal_blocker).expect("storage root");
    std::fs::write(internal_blocker.join("objects"), b"file").expect("objects blocker");
    assert!(matches!(
        StoragePaths::create(&internal_blocker),
        Err(StorageError::Unavailable)
    ));
}

#[cfg(unix)]
#[test]
fn path_creation_and_secure_parents_reject_symlinks() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("storage");
    let external = temporary.path().join("external");
    std::fs::create_dir(&root).expect("storage root");
    std::fs::create_dir(&external).expect("external directory");
    symlink(&external, root.join("objects")).expect("objects symlink");
    assert!(matches!(
        StoragePaths::create(&root),
        Err(StorageError::InvalidInput)
    ));

    let secure_root = temporary.path().join("secure");
    std::fs::create_dir(&secure_root).expect("secure root");
    symlink(&external, secure_root.join("linked")).expect("parent symlink");
    assert_eq!(
        secure_parent(&secure_root.join("linked/object.bin")),
        Err(StorageError::InvalidInput)
    );
}

#[test]
fn private_path_helpers_reject_missing_paths_and_accept_valid_uploads() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        reject_symlink(&temporary.path().join("missing")),
        Err(StorageError::Unavailable)
    );

    let paths = StoragePaths::create(&temporary.path().join("storage")).expect("paths");
    assert!(
        paths
            .upload(&MultipartId("valid-upload".to_owned()))
            .is_ok()
    );
}
