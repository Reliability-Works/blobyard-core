use super::{MapEnvironment, flags};
use blobyard_cli::{ConfigLoader, ConfigPaths, ConfigSource};
use blobyard_core::ErrorCode;
use std::fs;

#[test]
fn selects_self_hosted_profiles_with_isolated_endpoints_scope_and_credentials() {
    let directory = tempfile::tempdir().expect("tempdir");
    let user_file = directory.path().join("user/config.toml");
    fs::create_dir_all(user_file.parent().expect("user parent")).expect("user directory");
    fs::write(
        &user_file,
        concat!(
            "profile = \"local\"\n",
            "[profiles.local]\n",
            "api_url = \"http://127.0.0.1:8787/v1\"\n",
            "web_yard_origin = \"http://localhost:8787\"\n",
            "workspace = \"local-workspace\"\n",
            "project = \"local-project\"\n",
        ),
    )
    .expect("profile config");
    let paths = ConfigPaths::new(directory.path(), &user_file);
    let config = ConfigLoader::new(paths.clone(), &MapEnvironment::default())
        .load(&flags())
        .expect("self-hosted profile");

    assert_eq!(config.profile().as_str(), "local");
    assert_eq!(config.profile_source(), ConfigSource::User);
    assert_eq!(config.api().api_base_url(), "http://127.0.0.1:8787/v1");
    assert_eq!(config.api_source(), ConfigSource::Profile);
    assert_eq!(config.web_yard_origin().as_str(), "http://localhost:8787");
    assert_eq!(
        config.workspace().map(ToString::to_string).as_deref(),
        Some("local-workspace")
    );
    assert_eq!(config.workspace_source(), Some(ConfigSource::Profile));
    assert_eq!(
        config.project().map(ToString::to_string).as_deref(),
        Some("local-project")
    );
    assert_eq!(config.project_source(), Some(ConfigSource::Profile));
    assert!(
        paths
            .credentials_file(config.profile())
            .ends_with("user/credentials.local")
    );
}

#[test]
fn profile_selection_obeys_flag_environment_project_user_precedence() {
    let directory = tempfile::tempdir().expect("tempdir");
    let project = directory.path().join("repo/nested");
    let user_file = directory.path().join("user/config.toml");
    fs::create_dir_all(&project).expect("project directory");
    fs::create_dir_all(user_file.parent().expect("user parent")).expect("user directory");
    fs::write(
        directory.path().join("repo/.blobyard.toml"),
        "profile = \"project-profile\"\n",
    )
    .expect("project config");
    fs::write(
        &user_file,
        concat!(
            "profile = \"user-profile\"\n",
            "[profiles.user-profile]\napi_url = \"https://user-profile.example/v1\"\nweb_yard_origin = \"https://yards.user-profile.example\"\n",
            "[profiles.project-profile]\napi_url = \"https://project-profile.example/v1\"\nweb_yard_origin = \"https://yards.project-profile.example\"\n",
            "[profiles.environment-profile]\napi_url = \"https://environment-profile.example/v1\"\nweb_yard_origin = \"https://yards.environment-profile.example\"\n",
            "[profiles.flag-profile]\napi_url = \"https://flag-profile.example/v1\"\nweb_yard_origin = \"https://yards.flag-profile.example\"\n",
        ),
    )
    .expect("user config");
    let paths = ConfigPaths::new(&project, &user_file);
    let environment = MapEnvironment::default().with("BLOBYARD_PROFILE", "environment-profile");
    let mut selected = flags();
    selected.profile = Some("flag-profile".into());
    let from_flag = ConfigLoader::new(paths.clone(), &environment)
        .load(&selected)
        .expect("flag profile");
    assert_eq!(from_flag.profile().as_str(), "flag-profile");
    assert_eq!(from_flag.profile_source(), ConfigSource::Flag);

    let from_environment = ConfigLoader::new(paths.clone(), &environment)
        .load(&flags())
        .expect("environment profile");
    assert_eq!(from_environment.profile().as_str(), "environment-profile");
    assert_eq!(from_environment.profile_source(), ConfigSource::Environment);

    let from_project = ConfigLoader::new(paths, &MapEnvironment::default())
        .load(&flags())
        .expect("project profile");
    assert_eq!(from_project.profile().as_str(), "project-profile");
    assert_eq!(from_project.profile_source(), ConfigSource::Project);
}

#[test]
fn rejects_missing_invalid_or_endpointless_self_hosted_profiles() {
    let directory = tempfile::tempdir().expect("tempdir");
    let user_file = directory.path().join("config.toml");
    let paths = ConfigPaths::new(directory.path(), &user_file);
    let cases = [
        ("profile = \"missing\"\n", "missing profile"),
        ("profile = \"bad profile\"\n", "invalid selected name"),
        (
            "profile = \"local\"\n[profiles.local]\nworkspace = \"workspace\"\n",
            "missing endpoint",
        ),
        (
            "[profiles.\"bad profile\"]\napi_url = \"https://example.test/v1\"\n",
            "invalid name",
        ),
    ];
    for (content, label) in cases {
        fs::write(&user_file, content).expect("profile fixture");
        let error = ConfigLoader::new(paths.clone(), &MapEnvironment::default())
            .load(&flags())
            .expect_err(label);
        assert_eq!(error.code(), ErrorCode::InvalidRequest, "{label}");
    }
}
