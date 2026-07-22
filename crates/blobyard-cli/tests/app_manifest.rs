//! Instrumented local application manifest command behavior.

#![allow(clippy::expect_used, reason = "spawning the test binary must succeed")]

use std::process::Command;

const BLOBYARD_BIN: &str = env!("CARGO_BIN_EXE_blobyard");

fn run(cwd: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(BLOBYARD_BIN)
        .current_dir(cwd)
        .args(arguments)
        .output()
        .expect("instrumented Blobyard binary must run")
}

#[test]
fn app_init_scaffolds_valid_manifest_and_refuses_overwrite() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let initialized = run(temporary.path(), &["app", "init"]);
    assert!(initialized.status.success());
    assert_eq!(
        String::from_utf8_lossy(&initialized.stdout),
        "Created blobyard.toml.\n"
    );
    assert!(temporary.path().join("blobyard.toml").is_file());

    let validated = run(temporary.path(), &["app", "validate"]);
    assert!(validated.status.success());
    let stdout = String::from_utf8_lossy(&validated.stdout);
    assert!(stdout.contains("Valid application manifest: my-app (blobyard-js-1)."));
    assert!(stdout.contains("0 roles"));

    let conflict = run(temporary.path(), &["app", "init"]);
    assert_eq!(conflict.status.code(), Some(13));
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("[CONFLICT]"));
}

#[test]
fn app_validate_accepts_a_path_and_json_output() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(
        temporary.path().join("custom.toml"),
        concat!(
            "schema_version = 1\n",
            "[application]\n",
            "name = \"custom-app\"\n",
            "runtime = \"blobyard-js-1\"\n",
            "[[buckets]]\n",
            "name = \"assets\"\n",
        ),
    )
    .expect("manifest fixture");
    let output = run(
        temporary.path(),
        &["app", "validate", "custom.toml", "--json"],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"custom-app\""));
    assert!(stdout.contains("\"buckets\":1"));
    assert!(output.stderr.is_empty());
}

#[test]
fn app_validate_prints_every_precise_failure_path() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(
        temporary.path().join("blobyard.toml"),
        concat!(
            "schema_version = 1\n",
            "[application]\n",
            "name = \"invalid-app\"\n",
            "runtime = \"blobyard-js-1\"\n",
            "[[jobs]]\n",
            "name = \"daily\"\n",
            "function = \"missing\"\n",
            "schedule = \"61 0 * * *\"\n",
            "timezone = \"Not/AZone\"\n",
        ),
    )
    .expect("invalid fixture");
    let output = run(temporary.path(), &["app", "validate"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    for path in ["jobs[0].function", "jobs[0].schedule", "jobs[0].timezone"] {
        assert!(stderr.contains(path), "missing path: {path}\n{stderr}");
    }
}
