//! Configuration discovery, precedence, validation, and redaction contracts.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_cli::{
    ConfigLoader, ConfigPaths, ConfigSource, Environment, GlobalArgs, ProcessEnvironment,
};
use blobyard_core::ErrorCode;
use std::collections::HashMap;
use std::fs;

#[path = "config/profiles.rs"]
mod profiles;
#[path = "config/yards.rs"]
mod yards;

#[derive(Debug, Default)]
struct MapEnvironment(HashMap<String, String>);

impl MapEnvironment {
    fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.to_owned(), value.to_owned());
        self
    }
}

impl Environment for MapEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

const fn flags() -> GlobalArgs {
    GlobalArgs {
        json: false,
        quiet: false,
        verbose: false,
        api_url: None,
        web_yard_origin: None,
        profile: None,
        workspace: None,
        project: None,
        retry_key: None,
    }
}

#[test]
fn resolves_default_and_exposes_platform_paths_without_credentials() {
    let directory = tempfile::tempdir().expect("tempdir");
    let paths = ConfigPaths::new(directory.path(), directory.path().join("user/config.toml"));
    let config = ConfigLoader::new(paths, &MapEnvironment::default())
        .load(&flags())
        .expect("default config");
    assert_eq!(config.api().api_base_url(), "https://api.blobyard.com/v1");
    assert_eq!(config.api_source(), ConfigSource::Default);
    assert_eq!(config.web_yard_origin().as_str(), "https://blobyard.app");
    assert_eq!(config.workspace(), None);
    assert_eq!(config.workspace_source(), None);
    assert_eq!(config.project(), None);
    assert_eq!(config.project_source(), None);
    assert_eq!(config.environment_token(), None);
    assert_eq!(config.token_source(), "credential_store");
    assert_eq!(config.project_file(), None);
    assert!(config.yards().is_empty());
}

#[test]
fn default_profile_preserves_the_cloud_identity() {
    let directory = tempfile::tempdir().expect("tempdir");
    let paths = ConfigPaths::new(directory.path(), directory.path().join("user/config.toml"));
    let config = ConfigLoader::new(paths, &MapEnvironment::default())
        .load(&flags())
        .expect("default config");
    assert_eq!(config.profile().as_str(), "cloud");
    assert_eq!(config.profile_source(), ConfigSource::Default);
}

#[test]
fn exposes_config_paths_and_process_environment_without_credentials() {
    let directory = tempfile::tempdir().expect("tempdir");
    let paths = ConfigPaths::new(directory.path(), directory.path().join("user/config.toml"));
    let config = ConfigLoader::new(paths.clone(), &MapEnvironment::default())
        .load(&flags())
        .expect("default config");
    assert_eq!(config.paths(), &paths);
    assert_eq!(paths.cwd(), directory.path());
    assert!(paths.user_config().ends_with("user/config.toml"));
    assert!(
        paths
            .credentials_file(config.profile())
            .ends_with("user/credentials")
    );
    assert!(!format!("{config:?}").contains("token="));
    assert_eq!(ProcessEnvironment.get("BLOBYARD_TEST_MISSING_VALUE"), None);
}

struct PrecedenceFiles {
    _directory: tempfile::TempDir,
    project: std::path::PathBuf,
    user_file: std::path::PathBuf,
}

impl PrecedenceFiles {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("tempdir");
        let project = directory.path().join("repo/nested");
        let user_file = directory.path().join("user/config.toml");
        fs::create_dir_all(&project).expect("project dirs");
        fs::create_dir_all(user_file.parent().expect("user parent")).expect("user dir");
        fs::write(
            directory.path().join("repo/.blobyard.toml"),
            "api_url = \"https://project.example/v1\"\nworkspace = \"project-workspace\"\nproject = \"project-project\"\n",
        )
        .expect("project config");
        fs::write(
            &user_file,
            "api_url = \"https://user.example/v1\"\nworkspace = \"user-workspace\"\nproject = \"user-project\"\n",
        )
        .expect("user config");
        Self {
            _directory: directory,
            project,
            user_file,
        }
    }
}

#[test]
fn applies_flag_and_environment_precedence() {
    let files = PrecedenceFiles::new();
    let paths = ConfigPaths::new(&files.project, &files.user_file);
    let environment = MapEnvironment::default()
        .with("BLOBYARD_API_URL", "https://env.example/v1")
        .with("BLOBYARD_WEB_YARD_ORIGIN", "https://yards.env.example")
        .with("BLOBYARD_WORKSPACE", "env-workspace")
        .with("BLOBYARD_PROJECT", "env-project")
        .with("BLOBYARD_TOKEN", "temporary-token");
    let mut selected = flags();
    selected.api_url = Some("https://flag.example/v1".into());
    selected.web_yard_origin = Some("https://yards.flag.example".into());
    selected.workspace = Some("flag-workspace".into());
    let config = ConfigLoader::new(paths, &environment)
        .load(&selected)
        .expect("precedence");
    assert_eq!(config.api().api_base_url(), "https://flag.example/v1");
    assert_eq!(config.api_source(), ConfigSource::Flag);
    assert_eq!(
        config.web_yard_origin().as_str(),
        "https://yards.flag.example"
    );
    assert_eq!(
        config.workspace().map(ToString::to_string).as_deref(),
        Some("flag-workspace")
    );
    assert_eq!(config.workspace_source(), Some(ConfigSource::Flag));
    assert_eq!(
        config.project().map(ToString::to_string).as_deref(),
        Some("env-project")
    );
    assert_eq!(config.project_source(), Some(ConfigSource::Environment));
    assert_eq!(
        config
            .environment_token()
            .map(blobyard_core::SecretString::expose_secret),
        Some("temporary-token")
    );
    assert_eq!(config.token_source(), "environment");
    assert!(
        config
            .project_file()
            .is_some_and(|path| path.ends_with("repo/.blobyard.toml"))
    );
}

#[test]
fn applies_environment_project_and_user_fallback_precedence() {
    let files = PrecedenceFiles::new();
    let environment_only = ConfigLoader::new(
        ConfigPaths::new(&files.project, &files.user_file),
        &MapEnvironment::default().with("BLOBYARD_API_URL", "https://env.example/v1"),
    )
    .load(&flags())
    .expect("environment precedence");
    assert_eq!(environment_only.api_source(), ConfigSource::Environment);
    assert_eq!(
        environment_only.workspace_source(),
        Some(ConfigSource::Project)
    );

    fs::remove_file(files.project.parent().expect("repo").join(".blobyard.toml"))
        .expect("remove project config");
    let user_only = ConfigLoader::new(
        ConfigPaths::new(&files.project, &files.user_file),
        &MapEnvironment::default(),
    )
    .load(&flags())
    .expect("user precedence");
    assert_eq!(user_only.api_source(), ConfigSource::User);
    assert_eq!(user_only.workspace_source(), Some(ConfigSource::User));
    assert_eq!(user_only.project_source(), Some(ConfigSource::User));
}

#[test]
fn source_labels_are_stable_and_complete() {
    let cases = [
        (ConfigSource::Flag, "flag"),
        (ConfigSource::Environment, "environment"),
        (ConfigSource::Project, "project"),
        (ConfigSource::User, "user"),
        (ConfigSource::Profile, "profile"),
        (ConfigSource::Default, "default"),
    ];
    for (source, label) in cases {
        assert_eq!(source.as_str(), label);
    }
}

#[test]
fn rejects_invalid_endpoints_slugs_tokens_and_config_files() {
    let directory = tempfile::tempdir().expect("tempdir");
    let user_file = directory.path().join("config.toml");
    let paths = ConfigPaths::new(directory.path(), &user_file);

    let invalid_environment = [
        ("BLOBYARD_API_URL", "http://remote.example/v1"),
        ("BLOBYARD_WEB_YARD_ORIGIN", "https://yards.example/path"),
        ("BLOBYARD_WORKSPACE", "bad workspace"),
        ("BLOBYARD_PROJECT", "-bad"),
        ("BLOBYARD_TOKEN", "line\nbreak"),
    ];
    for (key, value) in invalid_environment {
        let error = ConfigLoader::new(paths.clone(), &MapEnvironment::default().with(key, value))
            .load(&flags())
            .expect_err("invalid environment");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }

    for content in [
        "unknown = true\n",
        "workspace = 42\n",
        "workspace = \"bad workspace\"\n",
    ] {
        fs::write(&user_file, content).expect("config fixture");
        assert!(
            ConfigLoader::new(paths.clone(), &MapEnvironment::default())
                .load(&flags())
                .is_err()
        );
    }
    fs::write(&user_file, vec![b'x'; 65_537]).expect("oversized fixture");
    assert!(
        ConfigLoader::new(paths.clone(), &MapEnvironment::default())
            .load(&flags())
            .is_err()
    );
    fs::remove_file(&user_file).expect("remove file");
    fs::create_dir(&user_file).expect("directory fixture");
    assert!(
        ConfigLoader::new(paths, &MapEnvironment::default())
            .load(&flags())
            .is_err()
    );
}

#[test]
fn reports_safe_io_failures_without_exposing_file_contents() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir().expect("tempdir");
    let binary = directory.path().join("config.toml");
    fs::write(&binary, [0xff]).expect("binary fixture");
    let error = ConfigLoader::new(
        ConfigPaths::new(directory.path(), &binary),
        &MapEnvironment::default(),
    )
    .load(&flags())
    .expect_err("invalid utf8");
    assert_eq!(error.code(), ErrorCode::InternalError);
    assert!(!error.message().contains("255"));

    let invalid_path = std::path::PathBuf::from(OsString::from_vec(b"bad\0path".to_vec()));
    let error = ConfigLoader::new(
        ConfigPaths::new(directory.path(), invalid_path),
        &MapEnvironment::default(),
    )
    .load(&flags())
    .expect_err("invalid path");
    assert_eq!(error.code(), ErrorCode::InternalError);
}

#[test]
fn malformed_nearest_project_config_fails_before_user_fallback() {
    let directory = tempfile::tempdir().expect("tempdir");
    fs::write(directory.path().join(".blobyard.toml"), "unknown = true").expect("project config");
    let error = ConfigLoader::new(
        ConfigPaths::new(directory.path(), directory.path().join("missing.toml")),
        &MapEnvironment::default(),
    )
    .load(&flags())
    .expect_err("invalid project config");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
}
