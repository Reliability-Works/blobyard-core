#![allow(
    clippy::redundant_pub_crate,
    reason = "application and runner siblings share the local manifest executor"
)]

use crate::{AppCommand, AppValidateArgs, CommandResult};
use blobyard_core::{ApplicationManifest, BlobyardError, ErrorCode, ManifestCapabilityCounts};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

const MANIFEST_NAME: &str = "blobyard.toml";
const SCAFFOLD: &str = r#"schema_version = 1

[application]
name = "my-app"
runtime = "blobyard-js-1"

[frontend]
directory = "dist"
"#;

pub(crate) fn execute(command: &AppCommand, cwd: &Path) -> Result<CommandResult, BlobyardError> {
    match command {
        AppCommand::Init => init(cwd),
        AppCommand::Validate(arguments) => validate(cwd, arguments),
    }
}

fn init(cwd: &Path) -> Result<CommandResult, BlobyardError> {
    let path = cwd.join(MANIFEST_NAME);
    let mut file = open_new(&path)?;
    map_write(
        file.write_all(SCAFFOLD.as_bytes())
            .and_then(|()| file.sync_all()),
    )
    .map(|()| {
        CommandResult::local(
            serde_json::json!({ "path": MANIFEST_NAME }),
            format!("Created {MANIFEST_NAME}."),
        )
    })
}

fn validate(cwd: &Path, arguments: &AppValidateArgs) -> Result<CommandResult, BlobyardError> {
    let path = resolve(cwd, &arguments.path);
    let source = read(&path)?;
    let manifest = ApplicationManifest::parse_toml(&source).map_err(|errors| {
        BlobyardError::new(
            ErrorCode::InvalidRequest,
            format!("Application manifest is invalid:\n{errors}"),
        )
    })?;
    let counts = manifest.capability_counts();
    Ok(CommandResult::local(
        serde_json::json!({
            "path": arguments.path,
            "name": manifest.application.name,
            "runtime": manifest.application.runtime.as_str(),
            "capabilities": counts,
        }),
        success_summary(&manifest, counts),
    ))
}

fn success_summary(manifest: &ApplicationManifest, counts: ManifestCapabilityCounts) -> String {
    format!(
        concat!(
            "Valid application manifest: {} ({}).\n",
            "Declared capabilities: {} roles, {} databases, {} buckets, {} functions, ",
            "{} jobs, {} routes, {} secrets, {} network targets, {} email functions."
        ),
        manifest.application.name,
        manifest.application.runtime.as_str(),
        counts.roles,
        counts.databases,
        counts.buckets,
        counts.functions,
        counts.jobs,
        counts.routes,
        counts.secrets,
        counts.network_targets,
        counts.email_functions,
    )
}

fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    }
}

fn open_new(path: &Path) -> Result<File, BlobyardError> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let message =
                format!("{MANIFEST_NAME} already exists. Edit it instead of overwriting it.");
            Err(BlobyardError::new(ErrorCode::Conflict, message))
        }
        Err(_) => Err(local_write_error()),
    }
}

fn read(path: &Path) -> Result<String, BlobyardError> {
    std::fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            BlobyardError::new(
                ErrorCode::NotFound,
                format!(
                    "{} was not found. Check the path and try again.",
                    path.display()
                ),
            )
        } else {
            BlobyardError::new(
                ErrorCode::InternalError,
                format!(
                    "Blobyard couldn't read {}. Check permissions and try again.",
                    path.display()
                ),
            )
        }
    })
}

fn map_write<T>(result: std::io::Result<T>) -> Result<T, BlobyardError> {
    result.map_err(|_| local_write_error())
}

fn local_write_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InternalError,
        format!("Blobyard couldn't write {MANIFEST_NAME}. Check permissions and try again."),
    )
}

#[cfg(test)]
#[path = "app_manifest_tests.rs"]
mod tests;
