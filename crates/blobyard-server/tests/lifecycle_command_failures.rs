#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Standalone command failure-path coverage.

include!("lifecycle_support/mod.rs");
use std::process::Stdio;

#[test]
fn recovery_commands_propagate_operation_and_report_write_failures() {
    let blocked = tempfile::tempdir().expect("blocked parent");
    let blocked_data = blocked.path().join("data");
    std::fs::create_dir(&blocked_data).expect("data directory");
    std::fs::create_dir(blocked_data.join("metadata.sqlite3")).expect("database blocker");
    let blocked_data = blocked_data.to_str().expect("blocked path");
    let output = blocked.path().join("backup");
    assert!(
        !run(&[
            "backup",
            "--data-dir",
            blocked_data,
            "--output",
            output.to_str().expect("output path"),
        ])
        .status
        .success()
    );
    for command in ["upgrade-preflight", "rollback-preflight"] {
        assert!(!run(&[command, "--data-dir", blocked_data]).status.success());
    }

    let valid = tempfile::tempdir().expect("valid parent");
    let source = valid.path().join("source");
    let source = source.to_str().expect("source path");
    assert!(
        run(&["bootstrap-token", "--generate", "--data-dir", source])
            .status
            .success()
    );
    let backup = valid.path().join("backup");
    assert!(
        run(&[
            "backup",
            "--data-dir",
            source,
            "--output",
            backup.to_str().expect("backup path"),
        ])
        .status
        .success()
    );
    assert!(!closed_stdout(&[
        "backup",
        "--data-dir",
        source,
        "--output",
        valid.path().join("closed-backup").to_str().expect("path"),
    ]));
    assert!(!closed_stdout(&[
        "restore",
        "--input",
        backup.to_str().expect("backup path"),
        "--data-dir",
        valid.path().join("closed-restore").to_str().expect("path"),
    ]));
    for command in ["upgrade-preflight", "rollback-preflight"] {
        assert!(!closed_stdout(&[command, "--data-dir", source]));
    }
}

#[test]
fn hosted_migration_propagates_storage_and_source_failures_after_reading_stdin() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let unavailable = run_with_stdin(
        &[
            "hosted-migrate",
            "--source-url",
            "http://127.0.0.1:1",
            "--token-stdin",
            "--data-dir",
            path,
        ],
        b"byd_pat_fixture\n",
    );
    assert!(!unavailable.status.success());

    let invalid_storage = run_with_stdin(
        &[
            "hosted-migrate",
            "--source-url",
            "http://127.0.0.1:1",
            "--token-stdin",
            "--data-dir",
            path,
            "--storage",
            "s3",
            "--s3-bucket",
            "bucket",
        ],
        b"byd_pat_fixture\n",
    );
    assert!(!invalid_storage.status.success());

    let unreadable = std::fs::File::open(temporary.path()).expect("directory input");
    let status = server_command()
        .args([
            "hosted-migrate",
            "--source-url",
            "http://127.0.0.1:1",
            "--token-stdin",
            "--data-dir",
            path,
        ])
        .stdin(Stdio::from(unreadable))
        .status()
        .expect("server status");
    assert!(!status.success());
}

fn closed_stdout(arguments: &[&str]) -> bool {
    let mut child = server_command()
        .args(arguments)
        .stdout(Stdio::piped())
        .spawn()
        .expect("server command");
    drop(child.stdout.take());
    child.wait().expect("server status").success()
}
