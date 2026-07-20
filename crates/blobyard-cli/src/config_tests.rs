#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{ConfigPaths, ConfigSource, ResolvedConfig};
use blobyard_api_client::ApiClientConfig;
use blobyard_core::{ErrorCode, WebYardOrigin};
use std::path::PathBuf;

#[test]
fn system_path_parts_fail_closed_and_join_the_user_file() {
    let cwd = PathBuf::from("/work");
    let config = PathBuf::from("/config/blobyard");
    let paths = ConfigPaths::from_system_parts(Some(cwd.clone()), Some(config.clone()));
    assert_eq!(
        paths.map(|value| value.user_config().to_owned()),
        Ok(config.join("config.toml"))
    );
    assert_eq!(
        ConfigPaths::from_system_parts(None, Some(config))
            .expect_err("missing current directory")
            .code(),
        ErrorCode::InternalError
    );
    assert_eq!(
        ConfigPaths::from_system_parts(Some(cwd), None)
            .expect_err("missing config directory")
            .code(),
        ErrorCode::InternalError
    );
}

#[test]
fn mcp_scope_overrides_are_validated_without_mutating_the_base_config() {
    let base = ResolvedConfig {
        profile: blobyard_core::Slug::new("cloud").expect("profile"),
        profile_source: ConfigSource::Default,
        api: ApiClientConfig::new("https://api.blobyard.com/v1").expect("valid API"),
        api_source: ConfigSource::Default,
        web_yard_origin: WebYardOrigin::new("https://blobyard.app").expect("yard origin"),
        workspace: None,
        workspace_source: None,
        project: None,
        project_source: None,
        environment_token: None,
        project_file: None,
        yards: std::collections::BTreeMap::new(),
        paths: ConfigPaths::new(PathBuf::from("/work"), PathBuf::from("/config/config.toml")),
    };
    let scoped = base
        .with_scope(Some("team".to_owned()), Some("project".to_owned()))
        .expect("valid scope");
    assert_eq!(
        scoped.workspace().map(ToString::to_string).as_deref(),
        Some("team")
    );
    assert_eq!(
        scoped.project().map(ToString::to_string).as_deref(),
        Some("project")
    );
    assert!(base.workspace().is_none());
    assert_eq!(
        base.with_scope(Some("bad slug".to_owned()), None)
            .expect_err("invalid workspace")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        base.with_scope(None, Some("bad slug".to_owned()))
            .expect_err("invalid project")
            .code(),
        ErrorCode::InvalidRequest
    );
}
