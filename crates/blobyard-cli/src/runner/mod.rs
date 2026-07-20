mod admin;
mod confirmation;
mod dashboard;
mod deploy;
mod deploy_output;
mod deploy_selection;
mod dispatch;
mod local;
mod login;
mod mcp;
mod mcp_admin;
mod mcp_dashboard;
mod mcp_yards;
mod objects;
mod preview;
mod projects;
mod retry;
mod sharing;
mod transfers;
mod workspaces;
mod yards;

use crate::commands::Command;
use crate::{CommandResult, OutputMode, ResolvedConfig, RetryKey, TokenStore};
use blobyard_api_client::{
    ApiClient, ApiRequest, EmptyResponse, Endpoint, TokenPair, TokenRefreshRequest,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString, Slug};
use serde::Serialize;
use std::sync::Arc;
use transfers::progress::TransferProgress;

/// Executes parsed commands against explicit API, configuration, and credential seams.
pub struct Runner {
    pub(super) api: ApiClient,
    pub(super) config: ResolvedConfig,
    login_port: Arc<dyn login::LoginPort>,
    pub(super) token_store: Arc<dyn TokenStore>,
    transfer_progress: TransferProgress,
    output_mode: OutputMode,
    confirmation: Arc<dyn confirmation::ConfirmationPort>,
    retry_key: Option<RetryKey>,
}

impl Runner {
    /// Creates a command runner.
    #[must_use]
    pub fn new(api: ApiClient, config: ResolvedConfig, token_store: Arc<dyn TokenStore>) -> Self {
        Self {
            api,
            config,
            login_port: Arc::new(login::SystemLoginPort::default()),
            token_store,
            transfer_progress: TransferProgress::hidden(),
            output_mode: OutputMode::Human,
            confirmation: Arc::new(confirmation::SystemConfirmation),
            retry_key: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_test_confirmation(
        &mut self,
        interactive: bool,
        result: Result<bool, BlobyardError>,
    ) {
        self.confirmation = Arc::new(confirmation::FixedConfirmation {
            interactive,
            result,
        });
    }

    pub(crate) fn with_output_mode(mut self, mode: crate::OutputMode) -> Self {
        self.transfer_progress = TransferProgress::for_output(mode);
        self.output_mode = mode;
        self
    }

    /// Selects an opaque mutation retry key supplied by the CLI caller.
    #[must_use]
    pub fn with_retry_key(mut self, retry_key: Option<RetryKey>) -> Self {
        self.retry_key = retry_key;
        self
    }

    /// Executes one parsed command.
    ///
    /// # Errors
    ///
    /// Returns stable validation, authentication, API, or local persistence errors.
    #[inline(never)]
    pub async fn execute(&self, command: &Command) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Projects { .. } | Command::Inbox { .. } | Command::Retention { .. } => {
                self.execute_scoped_resource(command).await
            }
            Command::Login(_) | Command::Logout | Command::Whoami => {
                self.execute_session(command).await
            }
            Command::Upload(_) | Command::Download(_) => self.execute_transfer(command).await,
            Command::Ls(arguments) => self.list_objects(arguments).await,
            Command::Rm(arguments) => self.remove_object(arguments).await,
            Command::Share(_) | Command::Preview(_) => self.execute_capability(command).await,
            Command::Deploy(arguments) => self.deploy(arguments).await,
            Command::Yard { command } => self.execute_yard(command).await,
            Command::Init | Command::Completion(_) | Command::Mcp { .. } => {
                self.execute_local(command)
            }
            _ => self.execute_headless(command).await,
        }
    }

    async fn access_token(&self) -> Result<SecretString, BlobyardError> {
        if let Some(token) = self.config.environment_token() {
            return Ok(token.clone());
        }
        let refresh_token = self.token_store.load()?.ok_or_else(login::auth_required)?;
        if self.config.profile().as_str() != "cloud" {
            return Ok(refresh_token);
        }
        let request = ApiRequest::new(Endpoint::TokenRefresh)
            .with_json(TokenRefreshRequest { refresh_token }.into_json());
        let success = self
            .api
            .execute::<TokenPair>(request)
            .await
            .map_err(api_error)?;
        self.token_store.save(&success.data().refresh_token)?;
        Ok(success.data().access_token.clone())
    }

    async fn execute_authed<T>(
        &self,
        request: ApiRequest,
    ) -> Result<blobyard_api_client::ApiSuccess<T>, BlobyardError>
    where
        T: serde::de::DeserializeOwned,
    {
        let token = self.access_token().await?;
        self.api
            .execute(request.with_bearer(token))
            .await
            .map_err(api_error)
    }

    fn scope(&self) -> Result<(Slug, Slug), BlobyardError> {
        let workspace = self.config.workspace().cloned().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a workspace with --workspace or Blobyard configuration.",
            )
        })?;
        let project = self.config.project().cloned().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a project with --project or Blobyard configuration.",
            )
        })?;
        Ok((workspace, project))
    }

    async fn logout(&self) -> Result<CommandResult, BlobyardError> {
        let (token, delete_saved) = if let Some(token) = self.config.environment_token() {
            (token.clone(), false)
        } else {
            let token = self
                .token_store
                .load()?
                .ok_or_else(|| BlobyardError::from_code(ErrorCode::AuthRequired))?;
            (token, true)
        };
        let request = self.mutation(Endpoint::Logout).with_bearer(token);
        let success = self
            .api
            .execute::<EmptyResponse>(request)
            .await
            .map_err(api_error)?;
        if delete_saved {
            self.token_store.delete()?;
        }
        command_result(success.data(), "Signed out.", success.request_id())
    }

    async fn whoami(&self) -> Result<CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<blobyard_api_client::WhoAmIResponse>(ApiRequest::new(
                Endpoint::WhoAmI,
            ))
            .await?;
        let human = whoami_human(success.data());
        command_result(success.data(), human, success.request_id())
    }
}

fn validate_resource_name(name: &str, resource: &str) -> Result<(), BlobyardError> {
    let invalid = name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control);
    if invalid {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            format!("{resource} name must be 1-128 printable characters."),
        ))
    } else {
        Ok(())
    }
}

fn whoami_human(identity: &blobyard_api_client::WhoAmIResponse) -> String {
    let principal = identity.email.as_ref().map_or_else(
        || format!("{} ({})", identity.display_name, identity.principal_id),
        |email| {
            format!(
                "{} <{email}> ({})",
                identity.display_name, identity.principal_id
            )
        },
    );
    format!(
        "{principal}\nWorkspace: {} ({})\nScopes: {}",
        identity.default_workspace.name,
        identity.default_workspace.slug,
        identity.scopes.join(", ")
    )
}

impl std::fmt::Debug for Runner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Runner").finish_non_exhaustive()
    }
}

fn command_result(
    data: &impl Serialize,
    human: impl Into<String>,
    request_id: &str,
) -> Result<CommandResult, BlobyardError> {
    to_json(data).map(|json| CommandResult::new(json, human, Some(request_id.to_owned())))
}

fn local_result(
    data: &impl Serialize,
    human: impl Into<String>,
) -> Result<CommandResult, BlobyardError> {
    to_json(data).map(|json| CommandResult::local(json, human))
}

fn to_json(value: &impl Serialize) -> Result<serde_json::Value, BlobyardError> {
    serde_json::to_value(value).map_err(|_| BlobyardError::from_code(ErrorCode::InternalError))
}

fn api_error(error: blobyard_api_client::ApiCallError) -> BlobyardError {
    error.into_error()
}

#[cfg(test)]
mod tests {
    use super::{to_json, whoami_human};
    use crate::Command;
    use blobyard_api_client::{PrincipalType, WhoAmIDefaultWorkspace, WhoAmIResponse};
    use serde::Serialize;

    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("synthetic failure"))
        }
    }

    #[test]
    fn json_mapping_fails_closed() {
        assert!(to_json(&FailingSerialize).is_err());
    }

    #[tokio::test]
    async fn transfer_dispatch_fails_closed() {
        let fixture = super::login::tests::support::Fixture::new(&["blobyard", "whoami"], vec![]);
        assert!(
            fixture
                .runner
                .execute_transfer(&Command::Whoami)
                .await
                .is_err()
        );
        assert!(
            fixture
                .runner
                .execute_capability(&Command::Whoami)
                .await
                .is_err()
        );
        assert!(
            fixture
                .runner
                .execute_session(&Command::Init)
                .await
                .is_err()
        );
    }

    #[test]
    fn identity_copy_omits_email_punctuation_for_ci() {
        let workspace = WhoAmIDefaultWorkspace {
            id: "workspace_1".into(),
            name: "Builds".into(),
            slug: "builds".into(),
        };
        let cli = WhoAmIResponse {
            default_workspace: workspace.clone(),
            display_name: "Developer".into(),
            email: Some("developer@example.com".into()),
            principal_id: "user_1".into(),
            principal_type: PrincipalType::Cli,
            scopes: vec!["object:read".into()],
        };
        assert!(whoami_human(&cli).contains("Developer <developer@example.com> (user_1)"));
        let ci = WhoAmIResponse {
            default_workspace: workspace,
            display_name: "GitHub acme/artifacts".into(),
            email: None,
            principal_id: "machine_1".into(),
            principal_type: PrincipalType::Ci,
            scopes: vec!["upload".into()],
        };
        let output = whoami_human(&ci);
        assert!(output.starts_with("GitHub acme/artifacts (machine_1)"));
        assert!(!output.contains('<'));
    }
}
