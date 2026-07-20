use super::ConfigSource;
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(crate) const DEFAULT_PROFILE: &str = "cloud";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileLayer {
    pub(super) api_url: Option<String>,
    pub(super) web_yard_origin: Option<String>,
    pub(super) workspace: Option<String>,
    pub(super) project: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct SelectedProfile {
    pub(super) name: Slug,
    pub(super) source: ConfigSource,
    pub(super) layer: ProfileLayer,
}

pub(super) fn select_profile(
    selected: Option<(String, ConfigSource)>,
    profiles: &BTreeMap<String, ProfileLayer>,
) -> Result<SelectedProfile, BlobyardError> {
    validate_profile_names(profiles)?;
    let (value, source) =
        selected.unwrap_or_else(|| (DEFAULT_PROFILE.to_owned(), ConfigSource::Default));
    let name = Slug::new(value).map_err(|_error| invalid_profile())?;
    let layer = profiles.get(name.as_str()).cloned().unwrap_or_default();
    if name.as_str() != DEFAULT_PROFILE && !profiles.contains_key(name.as_str()) {
        return Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "The selected Blobyard profile does not exist in the user config.",
        ));
    }
    if name.as_str() != DEFAULT_PROFILE
        && (layer.api_url.is_none() || layer.web_yard_origin.is_none())
    {
        return Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "A self-hosted Blobyard profile must define api_url and web_yard_origin in the user config.",
        ));
    }
    Ok(SelectedProfile {
        name,
        source,
        layer,
    })
}

fn validate_profile_names(profiles: &BTreeMap<String, ProfileLayer>) -> Result<(), BlobyardError> {
    for name in profiles.keys() {
        Slug::new(name.clone()).map_err(|_error| invalid_profile())?;
    }
    Ok(())
}

fn invalid_profile() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "A Blobyard profile name is not valid. Use letters, numbers, '-' or '_'.",
    )
}
