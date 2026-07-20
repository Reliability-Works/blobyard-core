use super::{MapEnvironment, flags};
use blobyard_cli::{ConfigLoader, ConfigPaths};
use blobyard_core::ErrorCode;
use std::fs;

#[test]
fn parses_named_yards_strictly_and_resolves_relative_directories() {
    let directory = tempfile::tempdir().expect("tempdir");
    let repo = directory.path().join("repo");
    let nested = repo.join("nested");
    fs::create_dir_all(&nested).expect("nested");
    fs::write(
        repo.join(".blobyard.toml"),
        concat!(
            "workspace = \"team\"\nproject = \"web\"\n",
            "[yards.marketing]\ndirectory = \"apps/marketing/dist\"\nclean_urls = true\n",
            "[yards.dashboard]\ndirectory = \"/tmp/dashboard\"\nspa = true\n",
        ),
    )
    .expect("project config");
    let config = ConfigLoader::new(
        ConfigPaths::new(&nested, directory.path().join("missing.toml")),
        &MapEnvironment::default(),
    )
    .load(&flags())
    .expect("named yards");
    let marketing = config.yards().get("marketing").expect("marketing");
    assert_eq!(marketing.directory(), repo.join("apps/marketing/dist"));
    assert!(marketing.clean_urls());
    assert!(!marketing.spa());
    let dashboard = config.yards().get("dashboard").expect("dashboard");
    assert_eq!(
        dashboard.directory(),
        std::path::Path::new("/tmp/dashboard")
    );
    assert!(dashboard.spa());
    assert!(!dashboard.clean_urls());
}

#[test]
fn rejects_invalid_reserved_empty_and_unknown_yard_configuration() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config_file = directory.path().join(".blobyard.toml");
    let cases = [
        "[yards.\"bad name\"]\ndirectory = \"dist\"\n",
        "[yards.API]\ndirectory = \"dist\"\n",
        "[yards.Docs2]\ndirectory = \"dist\"\n",
        "[yards.foo_bar]\ndirectory = \"dist\"\n",
        "[yards.foo--bar]\ndirectory = \"dist\"\n",
        "[yards.site]\ndirectory = \"\"\n",
        "[yards.site]\ndirectory = \"dist\"\nunknown = true\n",
    ];
    for content in cases {
        fs::write(&config_file, content).expect("project config");
        let error = ConfigLoader::new(
            ConfigPaths::new(directory.path(), directory.path().join("missing.toml")),
            &MapEnvironment::default(),
        )
        .load(&flags())
        .expect_err("invalid named yard");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }
}
