#![allow(
    clippy::redundant_pub_crate,
    reason = "the private parent config module re-exports the Yard-name validator to runners"
)]

use super::{ConfigLayer, invalid_config};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One strictly parsed named Web Yard from project configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardConfig {
    directory: PathBuf,
    spa: bool,
    clean_urls: bool,
}

impl YardConfig {
    pub(crate) const fn from_parts(directory: PathBuf, spa: bool, clean_urls: bool) -> Self {
        Self {
            directory,
            spa,
            clean_urls,
        }
    }

    /// Returns the directory resolved relative to `.blobyard.toml`.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns whether SPA fallback is enabled for this Yard.
    #[must_use]
    pub const fn spa(&self) -> bool {
        self.spa
    }

    /// Returns whether clean HTML URLs are enabled for this Yard.
    #[must_use]
    pub const fn clean_urls(&self) -> bool {
        self.clean_urls
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct YardConfigLayer {
    directory: PathBuf,
    #[serde(default)]
    spa: bool,
    #[serde(default)]
    clean_urls: bool,
}

pub(super) fn resolve_yards(
    project_file: Option<&Path>,
    project: Option<&ConfigLayer>,
) -> Result<BTreeMap<String, YardConfig>, BlobyardError> {
    let root = project_file
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    project.map_or_else(
        || Ok(BTreeMap::new()),
        |layer| {
            layer
                .yards
                .iter()
                .map(|(name, yard)| resolve_yard(root, name, yard))
                .collect()
        },
    )
}

fn resolve_yard(
    root: &Path,
    name: &str,
    yard: &YardConfigLayer,
) -> Result<(String, YardConfig), BlobyardError> {
    validate_yard_name(name)?;
    if yard.directory.as_os_str().is_empty() {
        return Err(invalid_config());
    }
    let directory = if yard.directory.is_absolute() {
        yard.directory.clone()
    } else {
        root.join(&yard.directory)
    };
    Ok((
        name.to_owned(),
        YardConfig::from_parts(directory, yard.spa, yard.clean_urls),
    ))
}

pub(crate) fn validate_yard_name(value: &str) -> Result<Slug, BlobyardError> {
    const RESERVED: [&str; 20] = [
        "admin", "api", "app", "assets", "audit", "billing", "blog", "cdn", "cli", "docs",
        "inboxes", "mail", "members", "new", "projects", "settings", "shares", "status", "support",
        "www",
    ];
    let slug = Slug::new(value).map_err(|_error| invalid_yard_name())?;
    let canonical = value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.contains("--");
    if !canonical || RESERVED.contains(&value) {
        Err(invalid_yard_name())
    } else {
        Ok(slug)
    }
}

fn invalid_yard_name() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "The Web Yard name isn't valid or is reserved. Choose another project-unique slug.",
    )
}
