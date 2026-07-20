use super::config_file::ConfigLayer;
use super::config_profile::{ProfileLayer, select_profile};
use super::config_values::{choose, identity, resolve_slug};
use super::{ConfigSource, Environment, GlobalArgs};
use blobyard_api_client::{ApiClientConfig, DEFAULT_API_BASE_URL};
use blobyard_core::{BlobyardError, CLOUD_WEB_YARD_ORIGIN, Slug, WebYardOrigin};

pub(super) struct ResolvedConnection {
    pub(super) profile: Slug,
    pub(super) profile_source: ConfigSource,
    pub(super) api: ApiClientConfig,
    pub(super) api_source: ConfigSource,
    pub(super) web_yard_origin: WebYardOrigin,
    pub(super) workspace: Option<Slug>,
    pub(super) workspace_source: Option<ConfigSource>,
    pub(super) project: Option<Slug>,
    pub(super) project_source: Option<ConfigSource>,
}

pub(super) fn resolve_connection(
    flags: &GlobalArgs,
    environment: &dyn Environment,
    project: Option<&ConfigLayer>,
    user: Option<&ConfigLayer>,
) -> Result<ResolvedConnection, BlobyardError> {
    let selected_profile = select_profile(
        choose(
            flags.profile.clone(),
            environment.get("BLOBYARD_PROFILE"),
            project.and_then(|layer| layer.profile.clone()),
            user.and_then(|layer| layer.profile.clone())
                .map(|value| (value, ConfigSource::User)),
        ),
        &user
            .map(|layer| &layer.profiles)
            .cloned()
            .unwrap_or_default(),
    )?;
    let (api_text, api_source) =
        resolve_api(flags, environment, project, user, &selected_profile.layer);
    let web_yard_origin = resolve_web_yard_origin(flags, environment, &selected_profile.layer)?;
    let (workspace, workspace_source) = resolve_slug(
        "workspace",
        choose(
            flags.workspace.clone(),
            environment.get("BLOBYARD_WORKSPACE"),
            project.and_then(|layer| layer.workspace.clone()),
            sourced_profile(
                selected_profile.layer.workspace.clone(),
                user.and_then(|layer| layer.workspace.clone()),
            ),
        ),
    )?;
    let (resolved_project, project_source) = resolve_slug(
        "project",
        choose(
            flags.project.clone(),
            environment.get("BLOBYARD_PROJECT"),
            project.and_then(|layer| layer.project.clone()),
            sourced_profile(
                selected_profile.layer.project.clone(),
                user.and_then(|layer| layer.project.clone()),
            ),
        ),
    )?;
    Ok(ResolvedConnection {
        profile: selected_profile.name,
        profile_source: selected_profile.source,
        api: ApiClientConfig::new(api_text)?,
        api_source,
        web_yard_origin,
        workspace,
        workspace_source,
        project: resolved_project,
        project_source,
    })
}

fn resolve_web_yard_origin(
    flags: &GlobalArgs,
    environment: &dyn Environment,
    profile: &ProfileLayer,
) -> Result<WebYardOrigin, BlobyardError> {
    let value = flags
        .web_yard_origin
        .clone()
        .or_else(|| environment.get("BLOBYARD_WEB_YARD_ORIGIN"))
        .or_else(|| profile.web_yard_origin.clone())
        .unwrap_or_else(|| CLOUD_WEB_YARD_ORIGIN.to_owned());
    WebYardOrigin::new(value)
}

fn resolve_api(
    flags: &GlobalArgs,
    environment: &dyn Environment,
    project: Option<&ConfigLayer>,
    user: Option<&ConfigLayer>,
    profile: &ProfileLayer,
) -> (String, ConfigSource) {
    choose(
        flags.api_url.clone(),
        environment.get("BLOBYARD_API_URL"),
        project.and_then(|layer| layer.api_url.clone()),
        sourced_profile(
            profile.api_url.clone(),
            user.and_then(|layer| layer.api_url.clone()),
        ),
    )
    .map_or_else(
        || (DEFAULT_API_BASE_URL.to_owned(), ConfigSource::Default),
        identity,
    )
}

fn sourced_profile(
    profile: Option<String>,
    user: Option<String>,
) -> Option<(String, ConfigSource)> {
    profile
        .map(|value| (value, ConfigSource::Profile))
        .or_else(|| user.map(|value| (value, ConfigSource::User)))
}
