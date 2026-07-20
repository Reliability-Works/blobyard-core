//! Local init and application orchestration behavior.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{FakeStore, Fixture, TestEnvironment, result_json};
use blobyard_cli::{ApplicationDependencies, Cli, ConfigPaths, Environment, TokenStore, run_with};
use blobyard_core::ErrorCode;
use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn init_writes_valid_project_config_once() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "init",
            "--api-url",
            "http://localhost:3210/v1",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("init");
    assert_eq!(result_json(result)["data"]["path"], ".blobyard.toml");
    let content = std::fs::read_to_string(fixture.temp.path().join(".blobyard.toml"))
        .expect("project config");
    assert!(content.contains("api_url = \"http://localhost:3210/v1\""));
    assert!(content.contains("workspace = \"team\""));
    assert!(content.contains("project = \"app\""));
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("no overwrite")
            .code(),
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn init_persists_a_self_hosted_profile_without_copying_profile_defaults() {
    let temp = tempfile::tempdir().expect("tempdir");
    let user_file = temp.path().join("user/config.toml");
    std::fs::create_dir_all(user_file.parent().expect("user parent")).expect("user directory");
    std::fs::write(
        &user_file,
        concat!(
            "[profiles.local]\n",
            "api_url = \"http://127.0.0.1:8787/v1\"\n",
            "web_yard_origin = \"http://localhost:8787\"\n",
            "workspace = \"default\"\n",
            "project = \"demo\"\n",
        ),
    )
    .expect("user config");
    let output = run_with(
        Cli::try_parse_from(["blobyard", "init", "--profile", "local"]).expect("profile grammar"),
        ApplicationDependencies {
            paths: ConfigPaths::new(temp.path(), &user_file),
            environment: Arc::new(TestEnvironment::default()),
            token_store: Some(Arc::new(FakeStore::default())),
        },
    )
    .await;
    assert_eq!(output.exit_code, 0);
    let content =
        std::fs::read_to_string(temp.path().join(".blobyard.toml")).expect("project config");
    assert!(content.contains("profile = \"local\""));
    assert!(!content.contains("api_url"));
    assert!(!content.contains("workspace"));
    assert!(!content.contains("project ="));
}

#[tokio::test]
async fn init_requires_values_and_reports_safe_write_failures() {
    let empty = Fixture::new(&["blobyard", "init"], Vec::new(), None, None);
    assert_eq!(
        empty
            .runner
            .execute(&empty.command)
            .await
            .expect_err("values required")
            .code(),
        ErrorCode::InvalidRequest
    );

    let missing_parent = Fixture::new(
        &["blobyard", "init", "--workspace", "team"],
        Vec::new(),
        None,
        None,
    );
    std::fs::remove_dir_all(missing_parent.temp.path()).expect("remove fixture cwd");
    assert_eq!(
        missing_parent
            .runner
            .execute(&missing_parent.command)
            .await
            .expect_err("write failure")
            .code(),
        ErrorCode::InternalError
    );
}

#[tokio::test]
async fn application_handles_completion_config_failure_and_runner_results() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store: Arc<dyn TokenStore> = Arc::new(FakeStore::default());
    let completion = run_with(
        Cli::try_parse_from(["blobyard", "completion", "fish", "--json"])
            .expect("completion grammar"),
        ApplicationDependencies {
            paths: ConfigPaths::new(temp.path(), temp.path().join("user/config.toml")),
            environment: Arc::new(TestEnvironment::default()),
            token_store: Some(store.clone()),
        },
    )
    .await;
    assert_eq!(completion.exit_code, 0);
    assert!(completion.stdout.contains("\"shell\":\"fish\""));

    let invalid_file = temp.path().join("config.toml");
    std::fs::write(&invalid_file, "unknown = true").expect("invalid config");
    let invalid = run_with(
        Cli::try_parse_from(["blobyard", "login", "--json"]).expect("login grammar"),
        ApplicationDependencies {
            paths: ConfigPaths::new(temp.path(), invalid_file),
            environment: Arc::new(TestEnvironment::unavailable_api()),
            token_store: Some(store.clone()),
        },
    )
    .await;
    assert_eq!(invalid.exit_code, 2);

    let dependencies = ApplicationDependencies {
        paths: ConfigPaths::new(temp.path(), temp.path().join("missing.toml")),
        environment: Arc::new(TestEnvironment::unavailable_api()),
        token_store: Some(store),
    };
    assert!(format!("{dependencies:?}").contains("ApplicationDependencies"));
    let unavailable = run_with(
        Cli::try_parse_from(["blobyard", "login", "--verbose"]).expect("login grammar"),
        dependencies,
    )
    .await;
    assert_eq!(unavailable.exit_code, ErrorCode::NetworkError.exit_code());
    assert!(unavailable.stderr.contains("diagnostic api="));

    let environment =
        TestEnvironment::unavailable_api().with("BLOBYARD_TOKEN", "temporary-ci-token");
    let environment_output = run_with(
        Cli::try_parse_from(["blobyard", "login", "--verbose"]).expect("login grammar"),
        ApplicationDependencies {
            paths: ConfigPaths::new(temp.path(), temp.path().join("missing.toml")),
            environment: Arc::new(environment),
            token_store: Some(Arc::new(FakeStore::default())),
        },
    )
    .await;
    assert!(
        environment_output
            .stderr
            .contains("diagnostic token_source=environment")
    );
}

#[tokio::test]
async fn application_can_select_the_production_platform_or_explicit_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = run_with(
        Cli::try_parse_from(["blobyard", "login"]).expect("login grammar"),
        ApplicationDependencies {
            paths: ConfigPaths::new(temp.path(), temp.path().join("user/config.toml")),
            environment: Arc::new(TestEnvironment::unavailable_api()),
            token_store: None,
        },
    )
    .await;
    assert_eq!(output.exit_code, ErrorCode::NetworkError.exit_code());
}

#[test]
fn support_environment_is_read_only() {
    let environment = TestEnvironment(HashMap::from([("KEY".into(), "value".into())]));
    assert_eq!(environment.get("KEY").as_deref(), Some("value"));
}
