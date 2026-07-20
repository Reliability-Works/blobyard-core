//! Instrumented Blobyard binary behavior tests.

#![allow(clippy::expect_used, reason = "spawning the test binary must succeed")]

use std::io::Write;
use std::process::{Command, Stdio};

use blobyard_cli::{
    ApplicationDependencies, ConfigPaths, FILE_FALLBACK_WARNING, ProcessEnvironment,
};

const BLOBYARD_BIN: &str = env!("CARGO_BIN_EXE_blobyard");

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(BLOBYARD_BIN)
        .args(arguments)
        .output()
        .expect("instrumented Blobyard binary must run")
}

fn run_with_input(arguments: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(BLOBYARD_BIN)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("instrumented Blobyard binary must run");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("input write");
    child.wait_with_output().expect("Blobyard binary must stop")
}

#[test]
fn public_application_dependencies_debug_output_redacts_runtime_seams() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let dependencies = ApplicationDependencies {
        paths: ConfigPaths::new(temporary.path(), temporary.path().join("config.toml")),
        environment: std::sync::Arc::new(ProcessEnvironment),
        token_store: None,
    };

    let debug = format!("{dependencies:?}");
    assert!(debug.contains("ApplicationDependencies"));
    assert!(debug.contains("has_token_store_override: false"));
    assert!(!debug.contains("environment"));
}

#[test]
fn binary_displays_the_public_help_contract() {
    let output = run(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Secure artifact storage for developers."));
    assert!(stdout.contains("Usage: blobyard [OPTIONS] <COMMAND>"));
}

#[test]
fn binary_executes_local_completion_without_claiming_remote_work() {
    let output = run(&["completion", "zsh", "--json"]);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"ok\":true"));
    assert!(output.stderr.is_empty());
}

#[test]
fn binary_rejects_invalid_command_grammar() {
    let output = run(&["download", "blobyard://studio/default/app.zip"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("--output <PATH>"));
}

#[test]
fn profile_add_validates_standard_input_before_local_configuration() {
    let arguments = &["profiles", "add", "local", "--token-stdin", "--json"];
    let empty = run_with_input(arguments, b"");
    assert_eq!(empty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&empty.stdout).contains("INVALID_REQUEST"));

    let token = "b".repeat(43);
    let missing_api = run_with_input(arguments, token.as_bytes());
    assert_eq!(missing_api.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&missing_api.stdout);
    assert!(stdout.contains("profiles add requires --api-url"));
    assert!(!stdout.contains(&token));

    let oversized = run_with_input(arguments, &vec![b'b'; 16_385]);
    assert_eq!(oversized.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&oversized.stdout).contains("INVALID_REQUEST"));
}

#[test]
fn binary_serves_mcp_without_appending_cli_output() {
    let mut child = Command::new(BLOBYARD_BIN)
        .args(["mcp", "serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP server must start");
    let initialize = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",",
        "\"params\":{\"protocolVersion\":\"2025-11-25\"}}\n"
    );
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(initialize.as_bytes())
        .expect("initialize write");
    let output = child
        .wait_with_output()
        .expect("MCP server must stop on EOF");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    assert!(output.status.success());
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("\"name\":\"blobyard-mcp\""));
    assert!(!stdout.contains("\"ok\":true"));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
    assert!(stderr.is_empty() || stderr == format!("{FILE_FALLBACK_WARNING}\n"));
}
