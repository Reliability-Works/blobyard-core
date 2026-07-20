use super::{MAX_CONFIG_BYTES, PROJECT_CONFIG_NAME, config_profile, config_yards, invalid_config};
use blobyard_core::BlobyardError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfigLayer {
    pub(super) profile: Option<String>,
    pub(super) api_url: Option<String>,
    pub(super) workspace: Option<String>,
    pub(super) project: Option<String>,
    #[serde(default)]
    pub(super) profiles: BTreeMap<String, config_profile::ProfileLayer>,
    #[serde(default)]
    pub(super) yards: BTreeMap<String, config_yards::YardConfigLayer>,
}

pub(super) fn discover_project_file(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join(PROJECT_CONFIG_NAME))
        .find(|candidate| candidate.is_file())
}

pub(super) fn read_layer(path: Option<&Path>) -> Result<Option<ConfigLayer>, BlobyardError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(super::local_io_error()),
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(invalid_config());
    }
    let source = fs::read_to_string(path).map_err(|_| super::local_io_error())?;
    toml::from_str(&source)
        .map(Some)
        .map_err(|_| invalid_config())
}
