use super::{Runner, local_result};
use crate::config::{ConfigSource, DEFAULT_PROFILE};
use blobyard_core::{BlobyardError, ErrorCode};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;

impl Runner {
    pub(super) fn init_project(&self) -> Result<crate::CommandResult, BlobyardError> {
        let profile = (self.config.profile().as_str() != DEFAULT_PROFILE)
            .then(|| self.config.profile().to_string());
        let api_url = (!matches!(
            self.config.api_source(),
            ConfigSource::Default | ConfigSource::Profile
        ))
        .then(|| self.config.api().api_base_url().to_owned());
        let workspace = (!matches!(
            self.config.workspace_source(),
            None | Some(ConfigSource::Profile)
        ))
        .then(|| self.config.workspace().map(ToString::to_string))
        .flatten();
        let project = (!matches!(
            self.config.project_source(),
            None | Some(ConfigSource::Profile)
        ))
        .then(|| self.config.project().map(ToString::to_string))
        .flatten();
        if profile.is_none() && api_url.is_none() && workspace.is_none() && project.is_none() {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a profile, API URL, workspace, or project before running blobyard init.",
            ));
        }
        let layer = ProjectConfig {
            profile,
            api_url,
            workspace,
            project,
        };
        let path = self.config.paths().cwd().join(".blobyard.toml");
        map_encoding(toml::to_string(&layer))
            .and_then(|content| open_project_file(&path).map(|file| (file, content)))
            .and_then(|(mut file, content)| {
                map_write(
                    file.write_all(content.as_bytes())
                        .and_then(|()| file.sync_all()),
                )
            })
            .and_then(|()| {
                local_result(
                    &serde_json::json!({ "path": ".blobyard.toml" }),
                    "Created .blobyard.toml.",
                )
            })
    }
}

fn open_project_file(path: &std::path::Path) -> Result<std::fs::File, BlobyardError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                BlobyardError::new(
                    ErrorCode::Conflict,
                    ".blobyard.toml already exists. Edit it instead of overwriting it.",
                )
            } else {
                local_write_error()
            }
        })
}

#[derive(Serialize)]
struct ProjectConfig {
    profile: Option<String>,
    api_url: Option<String>,
    workspace: Option<String>,
    project: Option<String>,
}

fn local_write_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InternalError,
        "Blobyard couldn't write project configuration. Check permissions and try again.",
    )
}

fn map_encoding<T, E>(result: Result<T, E>) -> Result<T, BlobyardError> {
    result.map_err(|_| BlobyardError::from_code(ErrorCode::InternalError))
}

fn map_write<T, E>(result: Result<T, E>) -> Result<T, BlobyardError> {
    result.map_err(|_| local_write_error())
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
