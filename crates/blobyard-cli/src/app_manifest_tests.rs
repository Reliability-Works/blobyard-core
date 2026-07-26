#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{MANIFEST_NAME, SCAFFOLD, execute, map_write};
use crate::{AppCommand, AppValidateArgs};
use blobyard_core::ErrorCode;
use std::path::PathBuf;

#[test]
fn init_creates_the_static_scaffold_and_refuses_overwrite() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    execute(&AppCommand::Init, temporary.path()).expect("manifest scaffold");
    assert_eq!(
        std::fs::read_to_string(temporary.path().join(MANIFEST_NAME)).expect("manifest"),
        SCAFFOLD
    );
    assert_eq!(
        execute(&AppCommand::Init, temporary.path())
            .expect_err("existing manifest")
            .code(),
        ErrorCode::Conflict
    );
}

#[test]
fn validate_supports_relative_and_absolute_paths() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let manifest = temporary.path().join("application.toml");
    std::fs::write(&manifest, SCAFFOLD).expect("manifest fixture");
    for path in [PathBuf::from("application.toml"), manifest] {
        execute(
            &AppCommand::Validate(AppValidateArgs { path }),
            temporary.path(),
        )
        .expect("valid manifest");
    }
}

#[test]
fn validate_reports_every_manifest_failure_and_local_read_failure() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let invalid = temporary.path().join(MANIFEST_NAME);
    std::fs::write(
        &invalid,
        "schema_version = 2\nunknown = true\n[application]\nname = \"Bad\"\nruntime = \"wrong\"\n",
    )
    .expect("invalid fixture");
    let failure = execute(
        &AppCommand::Validate(AppValidateArgs {
            path: PathBuf::from(MANIFEST_NAME),
        }),
        temporary.path(),
    )
    .expect_err("invalid manifest");
    assert_eq!(failure.code(), ErrorCode::InvalidRequest);
    assert!(failure.message().contains("schema_version"));
    assert!(failure.message().contains("application.name"));
    assert!(failure.message().contains("application.runtime"));
    assert!(failure.message().contains("unknown"));

    let missing = AppCommand::Validate(AppValidateArgs {
        path: PathBuf::from("missing.toml"),
    });
    assert_eq!(
        execute(&missing, temporary.path())
            .expect_err("missing manifest")
            .code(),
        ErrorCode::NotFound
    );
    let directory = AppCommand::Validate(AppValidateArgs {
        path: PathBuf::from("."),
    });
    assert_eq!(
        execute(&directory, temporary.path())
            .expect_err("directory is unreadable as text")
            .code(),
        ErrorCode::InternalError
    );
}

#[test]
fn write_failures_are_redaction_safe() {
    let missing = tempfile::tempdir()
        .expect("temporary directory")
        .path()
        .join("gone");
    assert_eq!(
        execute(&AppCommand::Init, &missing)
            .expect_err("missing current directory")
            .code(),
        ErrorCode::InternalError
    );
    assert_eq!(map_write::<u8>(Ok(7)), Ok(7));
    assert_eq!(
        map_write::<u8>(Err(std::io::Error::other("private provider detail")))
            .expect_err("write failure")
            .code(),
        ErrorCode::InternalError
    );
}
