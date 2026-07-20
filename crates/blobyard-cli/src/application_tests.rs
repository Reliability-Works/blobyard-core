#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_api_client::{ApiCallError, ApiRequest, Transport, TransportFuture};
use blobyard_cli::{
    ApplicationDependencies, Cli, ConfigLoader, ConfigPaths, Environment, OutputOptions,
    RenderedOutput, ResolvedConfig, TokenStore, run_cli, run_with,
    test_seams::{run_discovered, run_prepared},
    write_output,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use clap::Parser;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Default)]
struct EmptyEnvironment;

impl Environment for EmptyEnvironment {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

fn dependencies(cwd: &Path, user_config: PathBuf) -> ApplicationDependencies {
    ApplicationDependencies {
        paths: ConfigPaths::new(cwd, user_config),
        environment: Arc::new(EmptyEnvironment),
        token_store: Some(Arc::new(EmptyStore)),
    }
}

#[tokio::test]
async fn mcp_rejects_cli_output_flags_before_opening_standard_io() {
    let temp = tempfile::tempdir().expect("tempdir");
    for flag in ["--json", "--quiet", "--verbose"] {
        let cli =
            Cli::try_parse_from(["blobyard", flag, "mcp", "serve", "--stdio"]).expect("grammar");
        let output = run_with(
            cli,
            dependencies(temp.path(), temp.path().join("user/config.toml")),
        )
        .await;
        assert_eq!(output.exit_code, ErrorCode::InvalidRequest.exit_code());
        assert!(
            format!("{}{}", output.stdout, output.stderr).contains("MCP standard input and output")
        );
    }
}

#[derive(Debug, Default)]
struct TokenEnvironment;

impl Environment for TokenEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        (key == "BLOBYARD_TOKEN").then(|| "temporary-ci-token".to_owned())
    }
}

#[derive(Debug, Default)]
struct EmptyStore;

impl TokenStore for EmptyStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        Ok(None)
    }

    fn save(&self, _token: &SecretString) -> Result<(), BlobyardError> {
        Ok(())
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        Ok(())
    }
}

#[tokio::test]
async fn production_discovery_executes_local_completion() {
    let cli = Cli::try_parse_from(["blobyard", "completion", "zsh", "--json"])
        .expect("completion grammar");

    let output = run_cli(cli).await;

    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("\"shell\":\"zsh\""));
    assert!(output.stderr.is_empty());
}

#[derive(Debug, Default)]
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("synthetic write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn output_writes_both_streams_or_returns_the_internal_exit_code() {
    let output = RenderedOutput {
        stdout: "out".into(),
        stderr: "err".into(),
        exit_code: 12,
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(write_output(&output, &mut stdout, &mut stderr), 12);
    assert_eq!(stdout, b"out");
    assert_eq!(stderr, b"err");
    assert_eq!(
        write_output(&output, &mut FailingWriter, &mut Vec::new()),
        ErrorCode::InternalError.exit_code()
    );
    assert_eq!(
        write_output(&output, &mut Vec::new(), &mut FailingWriter),
        ErrorCode::InternalError.exit_code()
    );
}

#[derive(Debug, Default)]
struct UnusedTransport;

impl Transport for UnusedTransport {
    fn send<'a>(&'a self, _request: &'a ApiRequest) -> TransportFuture<'a> {
        Box::pin(async {
            Err(ApiCallError::new(
                BlobyardError::from_code(ErrorCode::InternalError),
                blobyard_api_client::RetryAdvice::Never,
            ))
        })
    }
}

fn fixture_config(cli: &Cli, cwd: &std::path::Path) -> ResolvedConfig {
    fixture_config_with(cli, cwd, &EmptyEnvironment)
}

fn fixture_config_with(
    cli: &Cli,
    cwd: &std::path::Path,
    environment: &dyn Environment,
) -> ResolvedConfig {
    ConfigLoader::new(
        ConfigPaths::new(cwd, cwd.join("user/config.toml")),
        environment,
    )
    .load(&cli.global)
    .expect("config")
}

async fn prepared_output(
    cli: Cli,
    cwd: &std::path::Path,
    warning: Option<&'static str>,
    transport: Result<Arc<dyn Transport>, BlobyardError>,
) -> RenderedOutput {
    let config = fixture_config(&cli, cwd);
    let options = OutputOptions::from_flags(&cli.global);
    run_prepared(
        cli,
        config,
        Arc::new(EmptyStore),
        warning,
        options,
        transport,
    )
    .await
}

async fn prepared_output_with_environment(
    cli: Cli,
    cwd: &std::path::Path,
    environment: &dyn Environment,
) -> RenderedOutput {
    let config = fixture_config_with(&cli, cwd, environment);
    let options = OutputOptions::from_flags(&cli.global);
    run_prepared(
        cli,
        config,
        Arc::new(EmptyStore),
        None,
        options,
        Ok(Arc::new(UnusedTransport)),
    )
    .await
}

#[tokio::test]
async fn discovery_and_transport_failures_render_stable_errors() {
    let cli = Cli::try_parse_from(["blobyard", "login", "--json"]).expect("grammar");
    let discovery =
        run_discovered(cli, Err(BlobyardError::from_code(ErrorCode::InternalError))).await;
    assert_eq!(discovery.exit_code, ErrorCode::InternalError.exit_code());

    let temp = tempfile::tempdir().expect("tempdir");
    let cli = Cli::try_parse_from(["blobyard", "login", "--json"]).expect("grammar");
    let output = prepared_output(
        cli,
        temp.path(),
        None,
        Err(BlobyardError::from_code(ErrorCode::InternalError)),
    )
    .await;
    assert_eq!(output.exit_code, ErrorCode::InternalError.exit_code());
}

#[tokio::test]
async fn configured_application_reports_unreadable_configuration() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let config = temporary.path().join("user/config.toml");
    std::fs::create_dir_all(&config).expect("configuration blocker");
    let cli = Cli::try_parse_from(["blobyard", "login", "--json"]).expect("grammar");

    let output = run_with(cli, dependencies(temporary.path(), config)).await;

    assert_eq!(output.exit_code, ErrorCode::InvalidRequest.exit_code());
    assert!(output.stdout.contains("INVALID_REQUEST"));
    assert!(output.stderr.is_empty());
}

#[tokio::test]
async fn prepared_application_renders_warning_success_and_runner_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cli = Cli::try_parse_from(["blobyard", "init", "--workspace", "team", "--json"])
        .expect("grammar");
    let output = prepared_output(
        cli,
        temp.path(),
        Some("Credential fallback is active."),
        Ok(Arc::new(UnusedTransport)),
    )
    .await;
    assert_eq!(output.exit_code, 0);
    assert!(output.stderr.contains("Credential fallback"));

    let cli = Cli::try_parse_from(["blobyard", "login"]).expect("grammar");
    let output = prepared_output(cli, temp.path(), None, Ok(Arc::new(UnusedTransport))).await;
    assert_eq!(output.exit_code, ErrorCode::InternalError.exit_code());

    let cli = Cli::try_parse_from(["blobyard", "login", "--verbose"]).expect("grammar");
    let output = prepared_output_with_environment(cli, temp.path(), &TokenEnvironment).await;
    assert!(output.stderr.contains("token_source=environment"));
}

#[tokio::test]
async fn self_hosted_profiles_reject_cloud_commands_before_transport() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("user")).expect("user config directory");
    std::fs::write(
        temp.path().join("user/config.toml"),
        "[profiles.local]\napi_url = \"http://localhost:3210\"\nweb_yard_origin = \"http://localhost:3210\"\n",
    )
    .expect("user config");
    let cli = Cli::try_parse_from([
        "blobyard",
        "--profile",
        "local",
        "--json",
        "billing",
        "show",
    ])
    .expect("grammar");
    let output = prepared_output_with_environment(cli, temp.path(), &TokenEnvironment).await;
    assert_eq!(
        output.exit_code,
        ErrorCode::OperationUnsupported.exit_code()
    );
    let rendered = format!("{}{}", output.stdout, output.stderr);
    assert!(rendered.contains("OPERATION_UNSUPPORTED"));
    assert!(!rendered.contains("INTERNAL_ERROR"));
}
