use super::{MAX_CONFIG_BYTES, invalid_config, local_io_error};
use blobyard_api_client::ApiClientConfig;
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

pub(crate) fn write_self_hosted_profile(
    path: &Path,
    profile: &Slug,
    api: &ApiClientConfig,
    web_yard_origin: &str,
    workspace: &Slug,
) -> Result<(), BlobyardError> {
    let existing = read_existing(path)?;
    reject_existing_profile(&existing, profile)?;
    let profile_text = format!(
        "[profiles.{}]\napi_url = {}\nweb_yard_origin = {}\nworkspace = {}\n",
        profile.as_str(),
        toml_string(api.api_base_url()),
        toml_string(web_yard_origin),
        toml_string(workspace.as_str())
    );
    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let source = format!("{existing}{separator}{profile_text}");
    atomic_write(path, &source)
}

pub(crate) fn ensure_new_profile(path: &Path, profile: &Slug) -> Result<(), BlobyardError> {
    reject_existing_profile(&read_existing(path)?, profile)
}

fn read_existing(path: &Path) -> Result<String, BlobyardError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_CONFIG_BYTES => {
            fs::read_to_string(path).map_err(map_local_io)
        }
        Ok(_) => Err(invalid_config()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(map_local_io(error)),
    }
}

fn reject_existing_profile(source: &str, profile: &Slug) -> Result<(), BlobyardError> {
    if source.is_empty() {
        return Ok(());
    }
    let document = source.parse::<toml::Table>().map_err(map_invalid_config)?;
    let exists = document
        .get("profiles")
        .and_then(toml::Value::as_table)
        .is_some_and(|profiles| profiles.contains_key(profile.as_str()));
    if exists {
        Err(BlobyardError::new(
            ErrorCode::Conflict,
            "That Blob Yard profile already exists. Choose another name.",
        ))
    } else {
        Ok(())
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn atomic_write(path: &Path, source: &str) -> Result<(), BlobyardError> {
    atomic_write_with(path, source, AtomicWriteHooks::REAL)
}

#[derive(Clone, Copy)]
struct AtomicWriteHooks {
    create_directory: fn(&Path) -> std::io::Result<()>,
    create_temporary: fn(&Path) -> std::io::Result<NamedTempFile>,
    write: fn(&mut NamedTempFile, &[u8]) -> std::io::Result<()>,
    flush: fn(&mut NamedTempFile) -> std::io::Result<()>,
    sync: fn(&NamedTempFile) -> std::io::Result<()>,
    make_private: fn(&Path) -> Result<(), BlobyardError>,
}

impl AtomicWriteHooks {
    const REAL: Self = Self {
        create_directory,
        create_temporary,
        write: write_temporary,
        flush: flush_temporary,
        sync: sync_temporary,
        make_private,
    };
}

fn atomic_write_with(
    path: &Path,
    source: &str,
    hooks: AtomicWriteHooks,
) -> Result<(), BlobyardError> {
    if source.len() as u64 > MAX_CONFIG_BYTES {
        return Err(invalid_config());
    }
    let parent = path.parent().ok_or_else(local_io_error)?;
    (hooks.create_directory)(parent).map_err(map_local_io)?;
    let mut temporary = (hooks.create_temporary)(parent).map_err(map_local_io)?;
    (hooks.write)(&mut temporary, source.as_bytes()).map_err(map_local_io)?;
    (hooks.flush)(&mut temporary).map_err(map_local_io)?;
    (hooks.sync)(&temporary).map_err(map_local_io)?;
    (hooks.make_private)(temporary.path())?;
    temporary.persist(path).map_err(map_local_io)?;
    Ok(())
}

fn create_temporary(parent: &Path) -> std::io::Result<NamedTempFile> {
    NamedTempFile::new_in(parent)
}

fn create_directory(parent: &Path) -> std::io::Result<()> {
    fs::create_dir_all(parent)
}

fn write_temporary(temporary: &mut NamedTempFile, source: &[u8]) -> std::io::Result<()> {
    temporary.write_all(source)
}

fn flush_temporary(temporary: &mut NamedTempFile) -> std::io::Result<()> {
    temporary.flush()
}

fn sync_temporary(temporary: &NamedTempFile) -> std::io::Result<()> {
    temporary.as_file().sync_all()
}

#[cfg(unix)]
fn make_private(path: &Path) -> Result<(), BlobyardError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(map_local_io)
}

#[cfg(not(unix))]
fn make_private(_path: &Path) -> Result<(), BlobyardError> {
    Ok(())
}

fn map_local_io<T>(_error: T) -> BlobyardError {
    local_io_error()
}

fn map_invalid_config<T>(_error: T) -> BlobyardError {
    invalid_config()
}

#[cfg(test)]
#[path = "config_profile_write_failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{atomic_write, ensure_new_profile, make_private, write_self_hosted_profile};
    use blobyard_api_client::ApiClientConfig;
    use blobyard_core::{ErrorCode, Slug};
    use std::fs;

    fn fixture() -> (Slug, ApiClientConfig, Slug) {
        (
            Slug::new("local").expect("profile"),
            ApiClientConfig::new("http://localhost:8787").expect("API URL"),
            Slug::new("default").expect("workspace"),
        )
    }

    #[test]
    fn writer_preserves_existing_content_with_canonical_spacing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (profile, api, workspace) = fixture();
        for (index, existing) in [
            "",
            "theme = \"dark\"",
            "theme = \"dark\"\n",
            "theme = \"dark\"\n\n",
        ]
        .into_iter()
        .enumerate()
        {
            let path = temp.path().join(format!("case-{index}/config.toml"));
            if !existing.is_empty() {
                fs::create_dir_all(path.parent().expect("parent")).expect("directory");
                fs::write(&path, existing).expect("existing config");
            }
            write_self_hosted_profile(&path, &profile, &api, "http://localhost:8787", &workspace)
                .expect("profile write");
            let source = fs::read_to_string(&path).expect("written config");
            let separator = if existing.is_empty() || existing.ends_with("\n\n") {
                ""
            } else if existing.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            assert_eq!(
                source,
                format!(
                    "{existing}{separator}[profiles.local]\napi_url = \"http://localhost:8787/v1\"\nweb_yard_origin = \"http://localhost:8787\"\nworkspace = \"default\"\n"
                )
            );
        }
    }

    #[test]
    fn writer_rejects_conflicts_and_unsafe_existing_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (profile, api, workspace) = fixture();
        let path = temp.path().join("config.toml");
        fs::write(
            &path,
            "[profiles.local]\napi_url = \"http://localhost:8787/v1\"\n",
        )
        .expect("existing profile");
        assert_eq!(
            ensure_new_profile(&path, &profile)
                .expect_err("duplicate profile")
                .code(),
            ErrorCode::Conflict
        );
        assert_eq!(
            write_self_hosted_profile(&path, &profile, &api, "http://localhost:8787", &workspace,)
                .expect_err("duplicate profile write")
                .code(),
            ErrorCode::Conflict
        );

        fs::write(&path, "not = [valid").expect("malformed config");
        assert_eq!(
            ensure_new_profile(&path, &profile)
                .expect_err("malformed config")
                .code(),
            ErrorCode::InvalidRequest
        );

        fs::write(&path, [0xff]).expect("non-Unicode config");
        assert_eq!(
            ensure_new_profile(&path, &profile)
                .expect_err("non-Unicode config")
                .code(),
            ErrorCode::InternalError
        );

        fs::remove_file(&path).expect("remove fixture");
        fs::create_dir(&path).expect("directory fixture");
        assert_eq!(
            write_self_hosted_profile(&path, &profile, &api, "http://localhost:8787", &workspace,)
                .expect_err("directory config")
                .code(),
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn writer_maps_oversized_and_unwritable_paths_safely() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (profile, api, workspace) = fixture();
        let oversized = temp.path().join("oversized.toml");
        fs::write(&oversized, "x".repeat(65_537)).expect("oversized config");
        assert_eq!(
            ensure_new_profile(&oversized, &profile)
                .expect_err("oversized config")
                .code(),
            ErrorCode::InvalidRequest
        );

        let parent_file = temp.path().join("parent-file");
        fs::write(&parent_file, "not a directory").expect("parent file");
        assert_eq!(
            write_self_hosted_profile(
                &parent_file.join("config.toml"),
                &profile,
                &api,
                "http://localhost:8787",
                &workspace,
            )
            .expect_err("unwritable parent")
            .code(),
            ErrorCode::InternalError
        );

        let nearly_full = temp.path().join("nearly-full.toml");
        fs::write(
            &nearly_full,
            format!("value = \"{}\"\n", "x".repeat(65_470)),
        )
        .expect("large valid config");
        assert_eq!(
            write_self_hosted_profile(
                &nearly_full,
                &profile,
                &api,
                "http://localhost:8787",
                &workspace,
            )
            .expect_err("combined config is oversized")
            .code(),
            ErrorCode::InvalidRequest
        );

        let directory_target = temp.path().join("directory-target");
        fs::create_dir(&directory_target).expect("directory target");
        assert_eq!(
            atomic_write(&directory_target, "value = true\n")
                .expect_err("cannot replace directory")
                .code(),
            ErrorCode::InternalError
        );

        let missing = temp.path().join("missing-file");
        #[cfg(unix)]
        assert_eq!(
            make_private(&missing)
                .expect_err("missing permission target")
                .code(),
            ErrorCode::InternalError
        );
    }
}
