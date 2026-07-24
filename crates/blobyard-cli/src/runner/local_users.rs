use super::{Runner, command_result};
use crate::headless_commands::{CreateUserArgs, UserIdArgs, UsersCommand};
use blobyard_api_client::{ApiRequest, Endpoint};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::AdminToolCall;
use blobyard_mcp::Scope;
use serde_json::{Value, json};

impl Runner {
    pub(super) async fn execute_users_command(
        &self,
        command: &UsersCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            UsersCommand::List => {
                self.admin_users(
                    AdminToolCall::ListLocalUsers {
                        scope: Scope::default(),
                    },
                    None,
                )
                .await
            }
            UsersCommand::Create(arguments) => self.create_local_user(arguments).await,
            UsersCommand::ResetKey(arguments) => self.reset_local_user_login_key(arguments).await,
            UsersCommand::Deactivate(arguments) => {
                self.admin_users(
                    AdminToolCall::DeactivateLocalUser {
                        scope: Scope::default(),
                        user_id: arguments.user_id.clone(),
                        confirmed: true,
                    },
                    Some("Local user deactivated."),
                )
                .await
            }
        }
    }

    async fn admin_users(
        &self,
        call: AdminToolCall,
        message: Option<&str>,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self.execute_mcp_admin_success(call).await?;
        let human = message.map_or_else(|| format!("{:#}", success.data()), str::to_owned);
        command_result(success.data(), human, success.request_id())
    }

    async fn create_local_user(
        &self,
        arguments: &CreateUserArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_user_arguments(arguments)?;
        let workspace = self
            .config
            .workspace()
            .map(ToString::to_string)
            .ok_or_else(|| {
                BlobyardError::new(
                    ErrorCode::InvalidRequest,
                    "Creating a local user requires --workspace.",
                )
            })?;
        let mut body = serde_json::Map::from_iter([
            (
                "displayName".to_owned(),
                Value::String(arguments.display_name.clone()),
            ),
            ("workspace".to_owned(), Value::String(workspace)),
        ]);
        if let Some(email) = &arguments.email {
            body.insert("email".to_owned(), Value::String(email.clone()));
        }
        let request = self
            .mutation(Endpoint::CreateLocalUser)
            .with_json(Value::Object(body));
        self.reveal_login_key(request).await
    }

    async fn reset_local_user_login_key(
        &self,
        arguments: &UserIdArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let request = self
            .mutation(Endpoint::ResetLocalUserLoginKey)
            .with_json(json!({ "userId": arguments.user_id }));
        self.reveal_login_key(request).await
    }

    async fn reveal_login_key(
        &self,
        request: ApiRequest,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self.execute_authed::<Value>(request).await?;
        let key = success
            .data()
            .get("loginKey")
            .and_then(Value::as_str)
            .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        let human = format!("Sign-in key: {key}\nCopy this key now. It will not be shown again.");
        command_result(success.data(), human, success.request_id())
    }
}

fn validate_user_arguments(arguments: &CreateUserArgs) -> Result<(), BlobyardError> {
    let name = arguments.display_name.trim();
    let valid_name = !name.is_empty() && name.len() <= 80 && !name.chars().any(char::is_control);
    let valid_email = arguments.email.as_deref().is_none_or(|email| {
        email.contains('@')
            && email.len() <= 254
            && !email
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
    });
    if valid_name && valid_email {
        Ok(())
    } else {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "The display name or email is invalid.",
        ))
    }
}

#[cfg(test)]
#[path = "local_users_tests.rs"]
mod tests;
