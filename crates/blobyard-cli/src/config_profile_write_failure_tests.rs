#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{AtomicWriteHooks, atomic_write, atomic_write_with};
use blobyard_core::ErrorCode;

#[test]
fn atomic_writer_maps_each_pre_persist_failure_without_creating_a_target() {
    let temp = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        atomic_write(std::path::Path::new("/"), "value = true\n")
            .expect_err("path without parent")
            .code(),
        ErrorCode::InternalError
    );
    let real = AtomicWriteHooks::REAL;
    let failures = [
        AtomicWriteHooks {
            create_directory: fail_directory,
            ..real
        },
        AtomicWriteHooks {
            create_temporary: fail_temporary,
            ..real
        },
        AtomicWriteHooks {
            write: fail_write,
            ..real
        },
        AtomicWriteHooks {
            flush: fail_flush,
            ..real
        },
        AtomicWriteHooks {
            sync: fail_sync,
            ..real
        },
        AtomicWriteHooks {
            make_private: fail_private,
            ..real
        },
    ];
    for (index, hooks) in failures.into_iter().enumerate() {
        let path = temp.path().join(format!("failure-{index}/config.toml"));
        assert_eq!(
            atomic_write_with(&path, "value = true\n", hooks)
                .expect_err("injected write failure")
                .code(),
            ErrorCode::InternalError
        );
        assert!(!path.exists());
    }
}

fn failure() -> std::io::Error {
    std::io::Error::other("injected profile write failure")
}

fn fail_directory(_path: &std::path::Path) -> std::io::Result<()> {
    Err(failure())
}

fn fail_temporary(_path: &std::path::Path) -> std::io::Result<tempfile::NamedTempFile> {
    Err(failure())
}

fn fail_write(_temporary: &mut tempfile::NamedTempFile, _source: &[u8]) -> std::io::Result<()> {
    Err(failure())
}

fn fail_flush(_temporary: &mut tempfile::NamedTempFile) -> std::io::Result<()> {
    Err(failure())
}

fn fail_sync(_temporary: &tempfile::NamedTempFile) -> std::io::Result<()> {
    Err(failure())
}

fn fail_private(_path: &std::path::Path) -> Result<(), blobyard_core::BlobyardError> {
    Err(super::local_io_error())
}
