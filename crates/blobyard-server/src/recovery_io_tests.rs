#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    IoFault, add_size, copy_verified, create_stage, hash_file, hash_reader, open_secure_file,
    persist_stage, read_all, read_secure_file, set_private_file, stream_hash, stream_hash_from,
    validate_directory, with_fault, write_private_file,
};
use crate::recovery::RecoveryError;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

#[derive(Debug)]
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("write failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn secure_files_require_real_directories_normal_paths_and_regular_files() {
    let parent = tempfile::tempdir().expect("parent");
    let root = parent.path().join("root");
    fs::create_dir(&root).expect("root");
    write_private_file(&root.join("nested/file"), b"secret").expect("write");
    assert_eq!(
        read_secure_file(&root, Path::new("nested/file")).expect("read"),
        b"secret"
    );
    drop(open_secure_file(&root, Path::new("nested/file")).expect("open"));

    assert_eq!(
        open_secure_file(&root, Path::new("/absolute")).expect_err("absolute path must fail"),
        RecoveryError::InvalidBackup
    );
    assert_eq!(
        open_secure_file(&root, Path::new("nested/../file")).expect_err("parent segment must fail"),
        RecoveryError::InvalidBackup
    );
    assert_eq!(
        open_secure_file(&root, Path::new("missing")).expect_err("missing file must fail"),
        RecoveryError::InvalidBackup
    );
    assert_eq!(
        open_secure_file(&root, Path::new("nested")).expect_err("directory target must fail"),
        RecoveryError::InvalidBackup
    );

    symlink(root.join("nested/file"), root.join("linked-file")).expect("file symlink");
    assert_eq!(
        open_secure_file(&root, Path::new("linked-file")).expect_err("file symlink must fail"),
        RecoveryError::InvalidBackup
    );
    symlink(root.join("nested"), root.join("linked-dir")).expect("directory symlink");
    assert_eq!(
        open_secure_file(&root, Path::new("linked-dir/file"))
            .expect_err("directory symlink must fail"),
        RecoveryError::InvalidBackup
    );
    let linked_root = parent.path().join("linked-root");
    symlink(&root, &linked_root).expect("root symlink");
    assert_eq!(
        validate_directory(&linked_root),
        Err(RecoveryError::InstallationUnavailable)
    );
    assert_eq!(
        open_secure_file(&linked_root, Path::new("nested/file"))
            .expect_err("root symlink must fail"),
        RecoveryError::InvalidBackup
    );
    assert_eq!(
        validate_directory(&root.join("nested/file")),
        Err(RecoveryError::InstallationUnavailable)
    );
    assert_eq!(
        validate_directory(&parent.path().join("absent")),
        Err(RecoveryError::InstallationUnavailable)
    );
}

#[test]
fn private_writes_hashes_and_verified_copies_preserve_bytes_and_permissions() {
    let root = tempfile::tempdir().expect("root");
    let private = root.path().join("private/nested/value");
    write_private_file(&private, b"value").expect("private write");
    assert_eq!(
        fs::metadata(&private)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(private.parent().expect("parent"))
            .expect("parent metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        write_private_file(&private, b"replacement"),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(
        hash_file(&private).expect("hash"),
        crate::recovery::test_support::sha256(b"value")
    );
    assert_eq!(
        hash_file(&root.path().join("missing")),
        Err(RecoveryError::InvalidBackup)
    );

    let target = root.path().join("copy/object");
    let result = copy_verified(&mut std::io::Cursor::new(b"copied"), &target).expect("copy");
    assert_eq!(result.0, 6);
    assert_eq!(result.1, crate::recovery::test_support::sha256(b"copied"));
    assert_eq!(fs::read(&target).expect("copied bytes"), b"copied");
    assert_eq!(
        copy_verified(&mut std::io::Cursor::new(b"again"), &target),
        Err(RecoveryError::Persistence)
    );
    set_private_file(&target).expect("permissions");
    assert_eq!(
        set_private_file(&root.path().join("absent")),
        Err(RecoveryError::Persistence)
    );
}

#[test]
fn stream_failures_and_overflow_are_classified_without_partial_success() {
    assert_eq!(
        read_all(&mut crate::test_support::FailingReader),
        Err(RecoveryError::InvalidBackup)
    );
    assert_eq!(
        hash_reader(&mut crate::test_support::FailingReader),
        Err(RecoveryError::Storage)
    );
    assert_eq!(
        stream_hash(&mut std::io::Cursor::new(b"bytes"), &mut FailingWriter),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(add_size(2, 3), Ok(5));
    assert_eq!(add_size(u64::MAX, 1), Err(RecoveryError::Integrity));
    assert_eq!(
        stream_hash_from(
            &mut std::io::Cursor::new(b"x"),
            &mut std::io::sink(),
            u64::MAX,
        ),
        Err(RecoveryError::Integrity)
    );
}

#[test]
fn staging_is_private_atomic_and_cleans_up_failed_persistence() {
    let root = tempfile::tempdir().expect("root");
    let destination = root.path().join("backup");
    let stage = create_stage(&destination).expect("stage");
    let stage_path = stage.path().to_owned();
    assert_eq!(
        fs::metadata(&stage_path)
            .expect("stage metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    write_private_file(&stage_path.join("value"), b"ready").expect("stage value");
    persist_stage(stage, &destination).expect("persist");
    assert_eq!(
        fs::read(destination.join("value")).expect("value"),
        b"ready"
    );
    assert_eq!(
        create_stage(&destination).expect_err("existing destination must fail"),
        RecoveryError::DestinationExists
    );

    let blocked_parent = root.path().join("blocked-parent");
    fs::write(&blocked_parent, b"file").expect("blocked parent");
    assert_eq!(
        create_stage(&blocked_parent.join("backup")).expect_err("blocked parent must fail"),
        RecoveryError::Persistence
    );
    assert_eq!(
        create_stage(Path::new("/")).expect_err("existing root must fail"),
        RecoveryError::DestinationExists
    );
    assert_eq!(
        create_stage(Path::new("")).expect_err("empty destination must fail"),
        RecoveryError::Persistence
    );

    let failed_destination = root.path().join("failed");
    let failed_stage = create_stage(&failed_destination).expect("failed stage");
    let failed_stage_path = failed_stage.path().to_owned();
    fs::create_dir(&failed_destination).expect("race destination");
    fs::write(failed_destination.join("occupied"), b"occupied").expect("occupy destination");
    assert_eq!(
        persist_stage(failed_stage, &failed_destination),
        Err(RecoveryError::Persistence)
    );
    assert!(!failed_stage_path.exists());
}

#[test]
fn write_helpers_reject_paths_without_usable_parent_directories() {
    assert_eq!(
        write_private_file(Path::new(""), b"value"),
        Err(RecoveryError::Persistence)
    );
    let relative = Path::new("blobyard-recovery-relative-fixture");
    let stage = create_stage(relative).expect("relative stage");
    drop(stage);
}

#[test]
fn deterministic_io_faults_cover_permission_sync_and_hash_failures() {
    let root = tempfile::tempdir().expect("root");
    assert_eq!(
        with_fault(IoFault::Permission, || {
            write_private_file(&root.path().join("permission"), b"value")
        }),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(
        with_fault(IoFault::Sync, || {
            write_private_file(&root.path().join("sync"), b"value")
        }),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(
        with_fault(IoFault::Permission, || {
            copy_verified(
                &mut std::io::Cursor::new(b"value"),
                &root.path().join("copy-permission"),
            )
        }),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(
        with_fault(IoFault::Sync, || {
            copy_verified(
                &mut std::io::Cursor::new(b"value"),
                &root.path().join("copy-sync"),
            )
        }),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(
        with_fault(IoFault::HashRead, || {
            hash_reader(&mut std::io::Cursor::new(b"value"))
        }),
        Err(RecoveryError::Storage)
    );
}

#[test]
fn deterministic_io_faults_cover_stage_and_secure_path_failures() {
    let root = tempfile::tempdir().expect("root");
    let blocked_parent = root.path().join("temp-parent");
    assert_eq!(
        with_fault(IoFault::BlockTempDirectory, || {
            create_stage(&blocked_parent.join("backup"))
        })
        .expect_err("blocked temp directory must fail"),
        RecoveryError::Persistence
    );
    let removed_stage = root.path().join("removed-stage");
    assert_eq!(
        with_fault(IoFault::RemoveStage, || create_stage(&removed_stage))
            .expect_err("removed stage must fail"),
        RecoveryError::Persistence
    );

    let secure_root = root.path().join("secure");
    fs::create_dir(&secure_root).expect("secure root");
    fs::write(secure_root.join("value"), b"value").expect("secure value");
    assert_eq!(
        with_fault(IoFault::RemoveSecureTarget, || {
            open_secure_file(&secure_root, Path::new("value"))
        })
        .expect_err("removed secure target must fail"),
        RecoveryError::InvalidBackup
    );
}
