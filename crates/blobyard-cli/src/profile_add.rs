use crate::config::{ensure_new_profile, write_self_hosted_profile};
use crate::{
    CommandResult, ConfigPaths, Diagnostics, GlobalArgs, OutputOptions, OutputRenderer,
    ProfileAddArgs, RenderedOutput, TokenStore, select_token_store,
};
use blobyard_api_client::{
    ApiClient, ApiClientConfig, ApiDeployment, BootstrapExchangeRequest, BootstrapExchangeResponse,
    Endpoint, ReqwestTransport, Transport,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString, Slug, SlugError, WebYardOrigin};
use std::io::Read;
use std::sync::Arc;

const MAX_BOOTSTRAP_TOKEN_BYTES: u64 = 16_384;

pub(super) async fn run_from_standard_input(
    global: &GlobalArgs,
    arguments: &ProfileAddArgs,
    paths: ConfigPaths,
    store: Option<Arc<dyn TokenStore>>,
    options: OutputOptions,
) -> RenderedOutput {
    let token = {
        let mut standard_input = std::io::stdin().lock();
        read_bootstrap_token(&mut standard_input)
    };
    match token {
        Ok(token) => run(global, arguments, paths, store, options, token, None).await,
        Err(error) => OutputRenderer::new(options, Diagnostics::default()).failure(&error),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "profile bootstrap keeps production and deterministic test seams explicit"
)]
pub(super) async fn run(
    global: &GlobalArgs,
    arguments: &ProfileAddArgs,
    paths: ConfigPaths,
    store: Option<Arc<dyn TokenStore>>,
    options: OutputOptions,
    token: SecretString,
    transport: Option<Arc<dyn Transport>>,
) -> RenderedOutput {
    let renderer = OutputRenderer::new(options, Diagnostics::default());
    render(
        renderer,
        prepare(
            global,
            arguments,
            &paths,
            store,
            token,
            TransportSelection::from(transport),
        )
        .await,
    )
}

fn render(
    renderer: OutputRenderer,
    outcome: Result<(CommandResult, Option<&'static str>), BlobyardError>,
) -> RenderedOutput {
    match outcome {
        Ok((result, warning)) => {
            let renderer = if let Some(warning) = warning {
                renderer.with_warning(warning)
            } else {
                renderer
            };
            renderer.success(result)
        }
        Err(error) => renderer.failure(&error),
    }
}

async fn prepare(
    global: &GlobalArgs,
    arguments: &ProfileAddArgs,
    paths: &ConfigPaths,
    store: Option<Arc<dyn TokenStore>>,
    token: SecretString,
    transport: TransportSelection,
) -> Result<(CommandResult, Option<&'static str>), BlobyardError> {
    let profile = profile_name(&arguments.name)?;
    let api = required_api(global)?;
    ensure_new_profile(paths.user_config(), &profile)?;
    let transport = select_transport(&api, transport)?;
    let response = ApiClient::for_deployment(transport, ApiDeployment::SelfHosted)
        .execute::<BootstrapExchangeResponse>(
            blobyard_api_client::ApiRequest::new(Endpoint::ExchangeBootstrapToken).with_json(
                BootstrapExchangeRequest {
                    name: format!("Blob Yard CLI profile {}", profile.as_str()),
                    platform: std::env::consts::OS.to_owned(),
                    token,
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                }
                .into_json(),
            ),
        )
        .await
        .map_err(blobyard_api_client::ApiCallError::into_error)?;
    let workspace = Slug::new(response.data().workspace.clone()).map_err(invalid_workspace)?;
    let web_yard_origin =
        WebYardOrigin::new(&response.data().web_yard_origin).map_err(|_error| {
            BlobyardError::new(
                ErrorCode::ProviderUnavailable,
                "The standalone service returned an invalid Web Yard origin.",
            )
        })?;
    let selected = selected_token_store(&profile, paths, store);
    selected.store().save(&response.data().access_token)?;
    if let Err(error) = write_self_hosted_profile(
        paths.user_config(),
        &profile,
        &api,
        web_yard_origin.as_str(),
        &workspace,
    ) {
        let _ = selected.store().delete();
        return Err(error);
    }
    let data = serde_json::json!({
        "apiUrl": api.api_base_url(),
        "profile": profile.as_str(),
        "scopes": response.data().scopes,
        "webYardOrigin": web_yard_origin.as_str(),
        "workspace": workspace.as_str(),
    });
    Ok((
        CommandResult::new(
            data,
            format!("Profile '{}' added.", profile.as_str()),
            Some(response.request_id().to_owned()),
        ),
        selected.warning(),
    ))
}

fn selected_token_store(
    profile: &Slug,
    paths: &ConfigPaths,
    store: Option<Arc<dyn TokenStore>>,
) -> crate::SelectedTokenStore {
    store.map_or_else(
        || select_token_store(profile, paths.credentials_file(profile)),
        crate::SelectedTokenStore::injected,
    )
}

fn select_transport(
    api: &ApiClientConfig,
    transport: TransportSelection,
) -> Result<Arc<dyn Transport>, BlobyardError> {
    match transport {
        TransportSelection::Automatic => ReqwestTransport::new(api.clone())
            .map(|transport| Arc::new(transport) as Arc<dyn Transport>),
        TransportSelection::Ready(transport) => Ok(transport),
        #[cfg(test)]
        TransportSelection::Failure(error) => Err(error),
    }
}

enum TransportSelection {
    Automatic,
    Ready(Arc<dyn Transport>),
    #[cfg(test)]
    Failure(BlobyardError),
}

impl From<Option<Arc<dyn Transport>>> for TransportSelection {
    fn from(transport: Option<Arc<dyn Transport>>) -> Self {
        transport.map_or(Self::Automatic, Self::Ready)
    }
}

fn invalid_workspace(_error: SlugError) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::ProviderUnavailable,
        "The standalone service returned an invalid workspace namespace.",
    )
}

fn profile_name(value: &str) -> Result<Slug, BlobyardError> {
    let profile = Slug::new(value.to_owned()).map_err(|_error| invalid_profile())?;
    if profile.as_str() == "cloud" {
        Err(BlobyardError::new(
            ErrorCode::Conflict,
            "The cloud profile is reserved. Choose another profile name.",
        ))
    } else {
        Ok(profile)
    }
}

fn required_api(global: &GlobalArgs) -> Result<ApiClientConfig, BlobyardError> {
    global.api_url.as_ref().map_or_else(
        || {
            Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "profiles add requires --api-url for the standalone service.",
            ))
        },
        ApiClientConfig::new,
    )
}

fn read_bootstrap_token(reader: &mut dyn Read) -> Result<SecretString, BlobyardError> {
    let mut source = String::new();
    reader
        .take(MAX_BOOTSTRAP_TOKEN_BYTES + 1)
        .read_to_string(&mut source)
        .map_err(|_| input_error())?;
    if source.len() as u64 > MAX_BOOTSTRAP_TOKEN_BYTES {
        return Err(input_error());
    }
    SecretString::new(source.trim_end_matches(['\r', '\n']).to_owned()).map_err(|_| input_error())
}

fn invalid_profile() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "A Blob Yard profile name is not valid. Use letters, numbers, '-' or '_'.",
    )
}

fn input_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "Standard input must contain one valid bootstrap token.",
    )
}

#[cfg(test)]
#[path = "profile_add_edge_tests.rs"]
mod tests;
