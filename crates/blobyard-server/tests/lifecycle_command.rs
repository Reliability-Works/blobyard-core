#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Standalone lifecycle command entry-path coverage.

include!("lifecycle_support/mod.rs");
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Output, Stdio};

#[test]
fn bootstrap_and_retention_commands_are_explicit_and_retry_safe() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let missing_confirmation = run(&["bootstrap-token", "--data-dir", path]);
    assert!(!missing_confirmation.status.success());
    assert!(stderr(&missing_confirmation).contains("requires --generate"));

    let generated = run(&["bootstrap-token", "--generate", "--data-dir", path]);
    assert!(generated.status.success());
    assert!(stderr(&generated).contains("shown once"));
    let repeated = run(&["bootstrap-token", "--generate", "--data-dir", path]);
    assert!(!repeated.status.success());
    assert!(stderr(&repeated).contains("already initialized or consumed"));

    let retention = run(&["retention-enforce", "--data-dir", path]);
    assert!(retention.status.success());
}

#[test]
fn reconcile_prints_one_deterministic_json_report() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let output = run(&["reconcile", "--data-dir", path]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("report JSON");
    assert_eq!(report["clean"], true);
    assert_eq!(report["counts"]["findings"], 0);
}

#[test]
fn backup_restore_and_compatibility_commands_complete_one_operator_journey() {
    let parent = tempfile::tempdir().expect("temporary directory");
    let source = parent.path().join("source");
    let backup = parent.path().join("backup");
    let restored = parent.path().join("restored");
    let source = source.to_str().expect("source path");
    let backup = backup.to_str().expect("backup path");
    let restored = restored.to_str().expect("restored path");
    assert!(
        run(&["bootstrap-token", "--generate", "--data-dir", source])
            .status
            .success()
    );

    let preflight = run(&["upgrade-preflight", "--data-dir", source]);
    assert!(preflight.status.success());
    assert!(
        preflight
            .stdout
            .windows(16)
            .any(|value| value == b"\"backupRequired\"")
    );
    assert!(
        run(&["rollback-preflight", "--data-dir", source])
            .status
            .success()
    );
    assert!(
        run(&["backup", "--data-dir", source, "--output", backup,])
            .status
            .success()
    );
    assert!(
        run(&["restore", "--input", backup, "--data-dir", restored,])
            .status
            .success()
    );
    let repeated = run(&["restore", "--input", backup, "--data-dir", restored]);
    assert!(!repeated.status.success());
    assert!(stderr(&repeated).contains("DestinationExists"));
}

#[test]
fn serve_rejects_an_unsafe_public_origin_before_binding() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let output = run(&[
        "serve",
        "--data-dir",
        path,
        "--public-url",
        "ftp://example.invalid",
    ]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("PublicOrigin"));
}

#[test]
fn serve_rejects_an_unsafe_web_yard_origin_before_binding() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let output = run(&[
        "serve",
        "--data-dir",
        path,
        "--web-yard-origin",
        "https://yards.example.com/path",
    ]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("WebYardOrigin"));
}

#[test]
fn lifecycle_commands_propagate_repository_initialization_failures() {
    for command in ["bootstrap-token", "retention-enforce", "reconcile"] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        std::fs::create_dir(temporary.path().join("metadata.sqlite3")).expect("database blocker");
        let path = temporary.path().to_str().expect("UTF-8 path");
        let arguments = if command == "bootstrap-token" {
            vec![command, "--generate", "--data-dir", path]
        } else {
            vec![command, "--data-dir", path]
        };
        assert!(!run(&arguments).status.success());
    }
}

#[test]
fn every_storage_aware_command_rejects_incomplete_s3_configuration() {
    for command in ["serve", "retention-enforce", "reconcile"] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().to_str().expect("UTF-8 path");
        let output = run(&[
            command,
            "--data-dir",
            path,
            "--storage",
            "s3",
            "--s3-bucket",
            "bucket",
        ]);
        assert!(!output.status.success());
        assert!(stderr(&output).contains("--s3-endpoint is required"));
    }
}

#[test]
fn recovery_commands_reject_incomplete_s3_configuration_before_data_access() {
    for (command, path_flag) in [("backup", "--output"), ("restore", "--input")] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().to_str().expect("UTF-8 path");
        let output = run(&[
            command,
            path_flag,
            path,
            "--storage",
            "s3",
            "--s3-bucket",
            "bucket",
        ]);
        assert!(!output.status.success());
        assert!(stderr(&output).contains("--s3-endpoint is required"));
    }
}

#[test]
fn reconcile_propagates_s3_open_failure_without_printing_a_report() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let output = server_command()
        .env("BLOBYARD_S3_ACCESS_KEY_ID", "access")
        .env("BLOBYARD_S3_SECRET_ACCESS_KEY", "secret")
        .args([
            "reconcile",
            "--data-dir",
            path,
            "--storage",
            "s3",
            "--s3-endpoint",
            "not-a-url",
            "--s3-bucket",
            "bucket",
        ])
        .output()
        .expect("server command");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn reconcile_propagates_a_closed_output_pipe() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let mut child = server_command()
        .args(["reconcile", "--data-dir", path])
        .stdout(Stdio::piped())
        .spawn()
        .expect("server command");
    drop(child.stdout.take());
    let status = child.wait().expect("server exit");
    assert!(!status.success());
}

#[test]
fn healthcheck_requires_a_successful_readiness_response() {
    let successful = health_response("200 OK");
    assert!(run(&["healthcheck", "--url", &successful]).status.success());

    let failing = health_response("503 Service Unavailable");
    assert!(!run(&["healthcheck", "--url", &failing]).status.success());
    assert!(
        !run(&["healthcheck", "--url", "http://127.0.0.1:1/v1/health",])
            .status
            .success()
    );
}

#[test]
fn hosted_migration_requires_a_bounded_nonempty_stdin_token() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().to_str().expect("UTF-8 path");
    let missing = run(&[
        "hosted-migrate",
        "--source-url",
        "http://127.0.0.1:1",
        "--data-dir",
        path,
    ]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("requires --token-stdin"));

    let empty = run_with_stdin(
        &[
            "hosted-migrate",
            "--source-url",
            "http://127.0.0.1:1",
            "--token-stdin",
            "--data-dir",
            path,
        ],
        b"\r\n",
    );
    assert!(!empty.status.success());

    let oversized = run_with_stdin(
        &[
            "hosted-migrate",
            "--source-url",
            "http://127.0.0.1:1",
            "--token-stdin",
            "--data-dir",
            path,
        ],
        &vec![b'a'; 16_385],
    );
    assert!(!oversized.status.success());
    assert!(stderr(&oversized).contains("source token input is too large"));
}

fn health_response(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let address = listener.local_addr().expect("listener address");
    std::thread::spawn(move || {
        let (mut stream, _peer) = listener.accept().expect("health request");
        let mut request = [0_u8; 1_024];
        let _read = stream.read(&mut request).expect("read health request");
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .expect("health response");
    });
    format!("http://{address}/v1/health")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}
