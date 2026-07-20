#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::download::{place_download, temporary_path, validate_destination};
use super::resume::{
    ResumeState, fail_save, load, remove, save, state_error_from, state_path, validate_permissions,
};
use std::path::Path;

#[test]
fn resume_and_download_local_failures_are_safe() {
    let temp = tempfile::tempdir().expect("temp");
    let corrupt = temp.path().join("corrupt.json");
    std::fs::write(&corrupt, b"not-json").expect("corrupt");
    make_private(&corrupt);
    assert!(load(&corrupt).is_err());
    let insecure = temp.path().join("insecure.json");
    std::fs::write(&insecure, b"{}").expect("insecure");
    make_public(&insecure);
    assert!(load(&insecure).is_err());
    assert!(load(temp.path()).is_err());
    assert!(remove(temp.path()).is_err());
    assert!(
        state_path(Path::new(""), "path")
            .to_string_lossy()
            .starts_with(".blobyard-resume-")
    );
    assert!(save(Path::new(""), &ResumeState::new("u".into(), "f".into(), 8)).is_err());

    assert!(validate_destination(temp.path(), true).is_err());
    assert!(temporary_path(&temp.path().join("missing/target")).is_err());
    let current = temporary_path(Path::new("relative.bin")).expect("relative temp");
    assert_eq!(current.parent(), Some(Path::new(".")));
    let missing_temp = temp.path().join("missing-temp");
    assert!(place_download(&missing_temp, &temp.path().join("target"), false).is_err());
}

#[test]
fn resume_persistence_maps_local_io_failures() {
    let temp = tempfile::tempdir().expect("temp");
    assert_eq!(
        state_error_from(std::io::Error::other("synthetic")).code(),
        blobyard_core::ErrorCode::StorageError
    );
    assert!(validate_permissions(&temp.path().join("missing")).is_err());
    let parent_file = temp.path().join("not-a-directory");
    std::fs::write(&parent_file, b"file").expect("parent file");
    let valid = ResumeState::new("u".into(), "a".repeat(64), 8 * 1024 * 1024);
    assert!(save(&parent_file.join("state.json"), &valid).is_err());
    let directory_target = temp.path().join("directory-target");
    std::fs::create_dir(&directory_target).expect("directory target");
    assert!(save(&directory_target, &valid).is_err());
    for step in 1..=6 {
        assert!(fail_save(&temp.path().join(format!("failure-{step}")), &valid, step).is_err());
    }
}

#[test]
fn resume_state_rejects_invalid_persisted_fields() {
    let temp = tempfile::tempdir().expect("temp");
    let fingerprint = "a".repeat(64);
    let mut cases = vec![
        ResumeState::new(String::new(), fingerprint.clone(), 8 * 1024 * 1024),
        ResumeState::new("u".into(), fingerprint.clone(), 1),
        ResumeState::new("u".into(), "short".into(), 8 * 1024 * 1024),
        ResumeState::new("u".into(), "z".repeat(64), 8 * 1024 * 1024),
        ResumeState::new("x".repeat(129), fingerprint.clone(), 8 * 1024 * 1024),
    ];
    let mut zero_part = ResumeState::new("u".into(), fingerprint.clone(), 8 * 1024 * 1024);
    zero_part.record(0, "etag".into());
    cases.push(zero_part);
    let mut invalid_etag = ResumeState::new("u".into(), fingerprint, 8 * 1024 * 1024);
    invalid_etag.record(1, "line\nbreak".into());
    cases.push(invalid_etag);
    for (index, state) in cases.iter().enumerate() {
        let path = temp.path().join(format!("state-{index}.json"));
        save(&path, state).expect("save invalid fixture");
        assert!(load(&path).is_err());
    }
}

#[cfg(unix)]
fn make_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("private");
}

#[cfg(not(unix))]
fn make_private(_path: &Path) {}

#[cfg(unix)]
fn make_public(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).expect("public");
}

#[cfg(not(unix))]
fn make_public(_path: &Path) {}
