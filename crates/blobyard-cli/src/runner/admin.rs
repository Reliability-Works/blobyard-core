use super::{Runner, command_result};
use crate::commands::Command;
use crate::headless_commands::{
    AuditCommand, InvitesCommand, MembersCommand, SessionsCommand, TokensCommand, TrustsCommand,
};
use blobyard_api_client::Endpoint;
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{AdminToolCall, Scope};
use serde_json::{Value, json};

const USER_API_SCOPES: [&str; 18] = [
    "workspace:read",
    "project:read",
    "project:write",
    "object:read",
    "object:write",
    "share:manage",
    "inbox:manage",
    "yard:read",
    "yard:manage",
    "retention:manage",
    "audit:read",
    "billing:manage",
    "account:export",
    "account:delete",
    "members:manage",
    "tokens:manage",
    "ci:manage",
    "sessions:manage",
];

impl Runner {
    pub(super) async fn execute_admin_command(
        &self,
        command: &Command,
    ) -> Result<crate::CommandResult, BlobyardError> {
        if let Command::Tokens {
            command: TokensCommand::Create(arguments),
        } = command
        {
            return self.create_api_token(arguments).await;
        }
        let call = admin_call(command)?;
        let success = self.execute_mcp_admin_success(call).await?;
        let human = admin_human(command, success.data());
        command_result(success.data(), human, success.request_id())
    }

    async fn create_api_token(
        &self,
        arguments: &crate::headless_commands::CreateTokenArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_token(arguments)?;
        let mut body = serde_json::Map::from_iter([
            ("name".to_owned(), Value::String(arguments.name.clone())),
            ("expiresInDays".to_owned(), json!(arguments.expires_days)),
            ("scopes".to_owned(), json!(arguments.scopes)),
        ]);
        if arguments.scopes.as_slice() == ["object:write"] {
            let workspace = self.config.workspace().ok_or_else(|| {
                BlobyardError::new(
                    ErrorCode::InvalidRequest,
                    "Cleanup tokens require both --workspace and --project.",
                )
            })?;
            let project = self.config.project().ok_or_else(|| {
                BlobyardError::new(
                    ErrorCode::InvalidRequest,
                    "Cleanup tokens require both --workspace and --project.",
                )
            })?;
            body.insert("workspace".to_owned(), json!(workspace));
            body.insert("project".to_owned(), json!(project));
        }
        let request = self
            .mutation(Endpoint::CreateApiToken)
            .with_json(Value::Object(body));
        let success = self.execute_authed::<Value>(request).await?;
        let token = success
            .data()
            .get("rawToken")
            .and_then(Value::as_str)
            .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        let human = format!("API token: {token}\nCopy this token now. It will not be shown again.");
        command_result(success.data(), human, success.request_id())
    }
}

fn admin_call(command: &Command) -> Result<AdminToolCall, BlobyardError> {
    let scope = Scope::default();
    match command {
        Command::Audit { .. } | Command::Members { .. } => audit_members_call(command, scope),
        Command::Invites { .. } | Command::Tokens { .. } => invites_tokens_call(command, scope),
        Command::Trusts { .. } | Command::Sessions { .. } => trusts_sessions_call(command, scope),
        _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
    }
}

fn audit_members_call(command: &Command, scope: Scope) -> Result<AdminToolCall, BlobyardError> {
    let call = match command {
        Command::Audit {
            command: AuditCommand::List(arguments),
        } => AdminToolCall::ListAudit {
            scope,
            cursor: arguments.cursor.clone(),
        },
        Command::Members {
            command: MembersCommand::List,
        } => AdminToolCall::ListMembers { scope },
        Command::Members {
            command: MembersCommand::Role(arguments),
        } => AdminToolCall::UpdateMemberRole {
            scope,
            user_id: arguments.user_id.clone(),
            role: arguments.role.as_str().to_owned(),
            confirmed: true,
        },
        Command::Members {
            command: MembersCommand::Remove(arguments),
        } => {
            if !arguments.force {
                return Err(BlobyardError::new(
                    ErrorCode::InvalidRequest,
                    "Pass --force to confirm member removal.",
                ));
            }
            AdminToolCall::RemoveMember {
                scope,
                user_id: arguments.user_id.clone(),
                confirmed: true,
            }
        }
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(call)
}

fn invites_tokens_call(command: &Command, scope: Scope) -> Result<AdminToolCall, BlobyardError> {
    let call = match command {
        Command::Invites {
            command: InvitesCommand::List,
        } => AdminToolCall::ListInvites { scope },
        Command::Invites {
            command: InvitesCommand::Create(arguments),
        } => AdminToolCall::CreateInvite {
            scope,
            email: arguments.email.clone(),
            role: arguments.role.as_str().to_owned(),
        },
        Command::Invites {
            command: InvitesCommand::Revoke(arguments),
        } => AdminToolCall::RevokeInvite {
            scope,
            invite_id: arguments.invite_id.clone(),
            confirmed: true,
        },
        Command::Tokens {
            command: TokensCommand::List,
        } => AdminToolCall::ListApiTokens { scope },
        Command::Tokens {
            command: TokensCommand::Revoke(arguments),
        } => AdminToolCall::RevokeApiToken {
            scope,
            token_id: arguments.token_id.clone(),
            confirmed: true,
        },
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(call)
}

fn trusts_sessions_call(command: &Command, scope: Scope) -> Result<AdminToolCall, BlobyardError> {
    let call = match command {
        Command::Trusts {
            command: TrustsCommand::List,
        } => AdminToolCall::ListCiTrusts { scope },
        Command::Trusts {
            command: TrustsCommand::Create(arguments),
        } => AdminToolCall::CreateCiTrust {
            scope,
            repository: arguments.repository.clone(),
            workflow_path: arguments.workflow_path.clone(),
            workflow_ref: arguments.workflow_ref.clone(),
            allowed_ref_glob: arguments.allowed_ref_glob.clone(),
            allowed_actions: arguments.allowed_actions.clone(),
            environment: arguments.environment.clone(),
        },
        Command::Trusts {
            command: TrustsCommand::Revoke(arguments),
        } => AdminToolCall::RevokeCiTrust {
            scope,
            trust_id: arguments.trust_id.clone(),
            confirmed: true,
        },
        Command::Sessions {
            command: SessionsCommand::List,
        } => AdminToolCall::ListCliSessions { scope },
        Command::Sessions {
            command: SessionsCommand::Revoke(arguments),
        } => AdminToolCall::RevokeCliSession {
            scope,
            session_id: arguments.session_id.clone(),
            confirmed: true,
        },
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(call)
}

fn admin_human(command: &Command, value: &Value) -> String {
    let message = match command {
        Command::Invites {
            command: InvitesCommand::Create(_),
        } => "Invitation created.",
        Command::Invites {
            command: InvitesCommand::Revoke(_),
        } => "Invitation revoked.",
        Command::Members {
            command: MembersCommand::Role(_),
        } => "Member role updated.",
        Command::Members {
            command: MembersCommand::Remove(_),
        } => "Member removed.",
        Command::Tokens {
            command: TokensCommand::Revoke(_),
        } => "API token revoked.",
        Command::Trusts {
            command: TrustsCommand::Create(_),
        } => "GitHub OIDC trust created.",
        Command::Trusts {
            command: TrustsCommand::Revoke(_),
        } => "GitHub OIDC trust revoked.",
        Command::Sessions {
            command: SessionsCommand::Revoke(_),
        } => "CLI session revoked.",
        _ => return format!("{value:#}"),
    };
    message.to_owned()
}

fn validate_token(
    arguments: &crate::headless_commands::CreateTokenArgs,
) -> Result<(), BlobyardError> {
    let valid_name = !arguments.name.trim().is_empty()
        && arguments.name.len() <= 80
        && !arguments.name.chars().any(char::is_control);
    let valid_scopes = !arguments.scopes.is_empty()
        && arguments.scopes.len() <= 20
        && arguments
            .scopes
            .iter()
            .all(|scope| USER_API_SCOPES.contains(&scope.as_str()));
    if valid_name && arguments.expires_days > 0 && valid_scopes {
        Ok(())
    } else {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Token name, lifetime, or scopes are invalid.",
        ))
    }
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
