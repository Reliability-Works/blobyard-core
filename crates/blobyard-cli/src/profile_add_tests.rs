#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_api_client::Transport;
use blobyard_cli::{
    Cli, ConfigPaths, OutputOptions, RenderedOutput, TokenStore,
    test_seams::{run_prepared, run_profile_add},
};
use blobyard_core::SecretString;
use clap::Parser;
use std::sync::Arc;

#[path = "profile_add_test_support.rs"]
mod support;
use support::{ConfigBlockingTransport, RecordingStore, RecordingTransport, fixture_config};

fn local_profile_add_cli() -> Cli {
    Cli::try_parse_from([
        "blobyard",
        "profiles",
        "add",
        "local",
        "--api-url",
        "http://localhost:8787",
        "--token-stdin",
        "--json",
    ])
    .expect("grammar")
}

fn bootstrap_token() -> SecretString {
    SecretString::new("b".repeat(43)).expect("bootstrap token")
}

fn local_profile_fixture() -> (tempfile::TempDir, ConfigPaths, Cli, Arc<RecordingStore>) {
    let temporary = tempfile::tempdir().expect("tempdir");
    let paths = ConfigPaths::new(temporary.path(), temporary.path().join("user/config.toml"));
    (
        temporary,
        paths,
        local_profile_add_cli(),
        Arc::new(RecordingStore::default()),
    )
}

async fn run_local_profile_add(
    cli: &Cli,
    paths: &ConfigPaths,
    store: &Arc<RecordingStore>,
    transport: Arc<dyn Transport>,
) -> RenderedOutput {
    run_profile_add(
        cli,
        paths.clone(),
        store.clone(),
        bootstrap_token(),
        transport,
    )
    .await
}

#[tokio::test]
async fn profile_add_exchanges_stdin_authority_and_persists_only_the_api_token() {
    let (temp, paths, cli, store) = local_profile_fixture();
    let transport = Arc::new(RecordingTransport::bootstrap());
    let output = run_local_profile_add(&cli, &paths, &store, transport.clone()).await;
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains(r#""profile":"local""#));
    assert!(!format!("{}{}", output.stdout, output.stderr).contains(&"a".repeat(43)));
    assert_eq!(
        store
            .load()
            .expect("stored token")
            .expect("token")
            .expose_secret(),
        "a".repeat(43)
    );
    let requests = transport.requests.lock().expect("request lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].endpoint(),
        blobyard_api_client::Endpoint::ExchangeBootstrapToken
    );
    drop(requests);
    let source = std::fs::read_to_string(paths.user_config()).expect("user config");
    assert!(source.contains("[profiles.local]"));
    assert!(source.contains("api_url = \"http://localhost:8787/v1\""));
    assert!(source.contains("web_yard_origin = \"http://localhost:8787\""));
    assert!(source.contains("workspace = \"default\""));
    let selected =
        Cli::try_parse_from(["blobyard", "--profile", "local", "whoami"]).expect("profile grammar");
    assert_eq!(
        fixture_config(&selected, temp.path()).profile().as_str(),
        "local"
    );
}

#[tokio::test]
async fn self_hosted_commands_use_the_stored_api_token_without_cloud_refresh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("user/config.toml");
    std::fs::create_dir_all(config_path.parent().expect("config parent"))
        .expect("user config directory");
    std::fs::write(
        config_path,
        "[profiles.local]\napi_url = \"http://localhost:8787/v1\"\nweb_yard_origin = \"http://localhost:8787\"\nworkspace = \"default\"\n",
    )
    .expect("user config");
    let cli = Cli::try_parse_from(["blobyard", "--profile", "local", "whoami", "--json"])
        .expect("grammar");
    let config = fixture_config(&cli, temp.path());
    let store = Arc::new(RecordingStore::default());
    store
        .save(&SecretString::new("a".repeat(43)).expect("API token"))
        .expect("save token");
    let transport = Arc::new(RecordingTransport::identity());
    let options = OutputOptions::from_flags(&cli.global);
    let output = run_prepared(cli, config, store, None, options, Ok(transport.clone())).await;
    assert_eq!(output.exit_code, 0);
    let requests = transport.requests.lock().expect("request lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].endpoint(),
        blobyard_api_client::Endpoint::WhoAmI
    );
    assert_eq!(
        requests[0].bearer().map(SecretString::expose_secret),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    drop(requests);
}

#[tokio::test]
async fn profile_add_rejects_invalid_setup_before_consuming_bootstrap_authority() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ConfigPaths::new(temp.path(), temp.path().join("user/config.toml"));
    std::fs::create_dir_all(temp.path().join("user")).expect("user config directory");
    std::fs::write(
        paths.user_config(),
        "[profiles.existing]\napi_url = \"http://localhost:8787/v1\"\n",
    )
    .expect("user config");
    let cases = [
        vec![
            "blobyard",
            "profiles",
            "add",
            "local",
            "--token-stdin",
            "--json",
        ],
        vec![
            "blobyard",
            "profiles",
            "add",
            "cloud",
            "--api-url",
            "http://localhost:8787",
            "--token-stdin",
            "--json",
        ],
        vec![
            "blobyard",
            "profiles",
            "add",
            "existing",
            "--api-url",
            "http://localhost:8787",
            "--token-stdin",
            "--json",
        ],
    ];
    let transport = Arc::new(RecordingTransport::default());
    for arguments in cases {
        let cli = Cli::try_parse_from(arguments).expect("grammar");
        let output = run_profile_add(
            &cli,
            paths.clone(),
            Arc::new(RecordingStore::default()),
            bootstrap_token(),
            transport.clone(),
        )
        .await;
        assert_ne!(output.exit_code, 0);
    }
    assert!(transport.requests.lock().expect("request lock").is_empty());
}

#[tokio::test]
async fn profile_add_test_seam_rejects_unrelated_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cli = Cli::try_parse_from(["blobyard", "completion", "zsh"]).expect("grammar");
    let output = run_profile_add(
        &cli,
        ConfigPaths::new(temp.path(), temp.path().join("user/config.toml")),
        Arc::new(RecordingStore::default()),
        bootstrap_token(),
        Arc::new(RecordingTransport::default()),
    )
    .await;
    assert_eq!(
        output.exit_code,
        blobyard_core::ErrorCode::InvalidRequest.exit_code()
    );
}

#[tokio::test]
async fn profile_add_rejects_invalid_workspace_without_persisting_the_access_token() {
    let (_temp, paths, cli, store) = local_profile_fixture();
    let output = run_local_profile_add(
        &cli,
        &paths,
        &store,
        Arc::new(RecordingTransport::bootstrap_workspace("bad workspace")),
    )
    .await;
    assert_eq!(
        output.exit_code,
        blobyard_core::ErrorCode::ProviderUnavailable.exit_code()
    );
    assert_eq!(store.load().expect("token store"), None);
}

#[tokio::test]
async fn profile_add_rejects_invalid_web_yard_origin_without_persisting_authority() {
    let (_temp, paths, cli, store) = local_profile_fixture();
    let output = run_local_profile_add(
        &cli,
        &paths,
        &store,
        Arc::new(RecordingTransport::bootstrap_origin(
            "https://yards.example/path",
        )),
    )
    .await;
    assert_eq!(
        output.exit_code,
        blobyard_core::ErrorCode::ProviderUnavailable.exit_code()
    );
    assert_eq!(store.load().expect("token store"), None);
    assert!(!paths.user_config().exists());
}

#[tokio::test]
async fn profile_add_removes_the_access_token_when_config_persistence_fails() {
    let (_temp, paths, cli, store) = local_profile_fixture();
    let config_path = paths.user_config().to_path_buf();
    let output = run_local_profile_add(
        &cli,
        &paths,
        &store,
        Arc::new(ConfigBlockingTransport::new(config_path)),
    )
    .await;
    assert_eq!(
        output.exit_code,
        blobyard_core::ErrorCode::InvalidRequest.exit_code()
    );
    assert_eq!(store.load().expect("token store"), None);
}
