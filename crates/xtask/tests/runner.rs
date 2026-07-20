//! Xtask command result tests against isolated workspaces.

#![allow(
    clippy::expect_used,
    reason = "isolated workspace fixtures must be created"
)]

use std::fs;
use xtask::{MAX_RUST_FILE_LINES, run};

#[test]
fn run_reports_usage_for_unknown_arguments() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let result = run(&[], directory.path());

    assert_eq!(result.exit_code(), 2);
    assert!(result.stdout().is_empty());
    assert_eq!(
        result.stderr(),
        "usage: cargo run -p xtask -- check-limits\n"
    );
}

#[test]
fn run_reports_success_for_a_compliant_workspace() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let source = directory.path().join("crates/example/src");
    fs::create_dir_all(&source).expect("source directory must be created");
    fs::write(source.join("lib.rs"), "pub fn valid() {}\n").expect("source must be written");

    let result = run(&["check-limits".to_owned()], directory.path());

    assert_eq!(result.exit_code(), 0);
    assert_eq!(result.stdout(), "Rust source limits passed.\n");
    assert!(result.stderr().is_empty());
}

#[test]
fn run_reports_limit_violations() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let source = directory.path().join("crates/example/src");
    fs::create_dir_all(&source).expect("source directory must be created");
    let oversized = "const VALUE: usize = 1;\n".repeat(MAX_RUST_FILE_LINES + 1);
    fs::write(source.join("long.rs"), oversized).expect("source must be written");

    let result = run(&["check-limits".to_owned()], directory.path());

    assert_eq!(result.exit_code(), 1);
    assert!(result.stdout().is_empty());
    assert!(
        result
            .stderr()
            .contains("crates/example/src/long.rs has 301")
    );
}

#[test]
fn run_reports_scan_errors() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let result = run(&["check-limits".to_owned()], directory.path());

    assert_eq!(result.exit_code(), 1);
    assert!(result.stdout().is_empty());
    assert!(
        result
            .stderr()
            .starts_with("failed to check Rust source limits:")
    );
}
