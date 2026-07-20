use super::Runner;
use crate::Command;
use crate::commands::{
    CreateInboxArgs, CreateProjectArgs, DownloadArgs, InboxCommand, ListArgs, McpServeArgs,
    PreviewArgs, ProjectsCommand, RemoveArgs, RetentionCommand, RevokeInboxArgs, SetRetentionArgs,
    ShareArgs, UploadArgs,
};
use crate::headless_commands::{
    CreateWorkspaceArgs, RevokePreviewArgs, RevokeShareArgs, WorkspacesCommand,
};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{BackendError, BackendFuture, Scope, ToolBackend, ToolCall};
use std::num::NonZeroU32;
use std::path::PathBuf;

#[cfg(test)]
use blobyard_api_client::Endpoint;

impl Runner {
    pub(crate) async fn serve_mcp(
        &self,
        _arguments: &McpServeArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        finish_mcp(blobyard_mcp::serve_stdio(self).await)
    }

    pub(super) fn mcp_scope(&self, scope: Scope) -> Result<Self, BlobyardError> {
        Ok(Self {
            api: self.api.clone(),
            config: self.config.with_scope(scope.workspace, scope.project)?,
            login_port: self.login_port.clone(),
            token_store: self.token_store.clone(),
            transfer_progress: self.transfer_progress,
            output_mode: self.output_mode,
            confirmation: self.confirmation.clone(),
            retry_key: self.retry_key.clone(),
        })
    }

    async fn execute_mcp(&self, call: ToolCall) -> Result<serde_json::Value, BlobyardError> {
        match call {
            ToolCall::Admin(call) => return self.execute_mcp_admin(call).await,
            ToolCall::ListShares { scope } => {
                return self
                    .mcp_scope(scope)?
                    .list_shares()
                    .await
                    .map(crate::CommandResult::into_data);
            }
            ToolCall::ListPreviews { scope } => {
                return self
                    .mcp_scope(scope)?
                    .list_previews()
                    .await
                    .map(crate::CommandResult::into_data);
            }
            ToolCall::RevokeShare { scope, share_id } => {
                return self
                    .mcp_scope(scope)?
                    .revoke_share(&RevokeShareArgs { share_id })
                    .await
                    .map(crate::CommandResult::into_data);
            }
            ToolCall::RevokePreview { scope, preview_id } => {
                return self
                    .mcp_scope(scope)?
                    .revoke_preview(&RevokePreviewArgs { preview_id })
                    .await
                    .map(crate::CommandResult::into_data);
            }
            _ => {}
        }
        let (scope, command) = mcp_command(call)?;
        self.mcp_scope(scope)?
            .execute(&command)
            .await
            .map(crate::CommandResult::into_data)
    }
}

impl ToolBackend for Runner {
    fn call(&self, call: ToolCall) -> BackendFuture<'_> {
        Box::pin(async move {
            self.execute_mcp(call)
                .await
                .map_err(|error| backend_error(&error))
        })
    }
}

fn mcp_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    if matches!(&call, ToolCall::Admin(_)) {
        return Err(BlobyardError::from_code(ErrorCode::InternalError));
    }
    if matches!(&call, ToolCall::WebYard(_)) {
        return super::mcp_yards::mcp_yard_command(call);
    }
    if matches!(&call, ToolCall::Dashboard(_)) {
        return super::mcp_dashboard::mcp_dashboard_command(call);
    }
    if matches!(
        &call,
        ToolCall::Whoami { .. }
            | ToolCall::ListWorkspaces { .. }
            | ToolCall::CreateWorkspace { .. }
            | ToolCall::ListProjects { .. }
            | ToolCall::ListObjects { .. }
            | ToolCall::GetRetention { .. }
            | ToolCall::ListInboxes { .. }
            | ToolCall::CreateProject { .. }
    ) {
        return mcp_resource_command(call);
    }
    if matches!(
        &call,
        ToolCall::UploadFile { .. } | ToolCall::DownloadFile { .. } | ToolCall::DeleteObject { .. }
    ) {
        return mcp_transfer_command(call);
    }
    mcp_capability_command(call)
}

fn mcp_resource_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        ToolCall::Whoami { scope } => (scope, Command::Whoami),
        ToolCall::ListWorkspaces { scope } => (
            scope,
            Command::Workspaces {
                command: WorkspacesCommand::List,
            },
        ),
        ToolCall::CreateWorkspace { scope, name } => (
            scope,
            Command::Workspaces {
                command: WorkspacesCommand::Create(CreateWorkspaceArgs { name }),
            },
        ),
        ToolCall::ListProjects { scope } => (
            scope,
            Command::Projects {
                command: ProjectsCommand::List,
            },
        ),
        ToolCall::ListObjects {
            scope,
            prefix,
            versions,
        } => (scope, Command::Ls(ListArgs { prefix, versions })),
        ToolCall::GetRetention { scope } => (
            scope,
            Command::Retention {
                command: RetentionCommand::Show,
            },
        ),
        ToolCall::ListInboxes { scope } => (
            scope,
            Command::Inbox {
                command: InboxCommand::List,
            },
        ),
        ToolCall::CreateProject { scope, name } => (
            scope,
            Command::Projects {
                command: ProjectsCommand::Create(CreateProjectArgs { name }),
            },
        ),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

fn mcp_transfer_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        ToolCall::UploadFile {
            scope,
            source,
            path,
            include_ignored,
        } => (
            scope,
            Command::Upload(UploadArgs {
                source: PathBuf::from(source),
                path,
                include_ignored,
            }),
        ),
        ToolCall::DownloadFile {
            scope,
            uri,
            output,
            force,
        } => (
            scope,
            Command::Download(DownloadArgs {
                uri,
                output: PathBuf::from(output),
                force,
            }),
        ),
        ToolCall::DeleteObject { scope, uri } => (scope, Command::Rm(RemoveArgs { uri })),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

fn mcp_capability_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        ToolCall::CreateShare {
            scope,
            target,
            expires,
            notify,
        } => (
            scope,
            Command::Share(ShareArgs {
                target,
                expires,
                notify,
            }),
        ),
        ToolCall::CreatePreview {
            scope,
            directory,
            expires,
        } => (
            scope,
            Command::Preview(PreviewArgs {
                directory: PathBuf::from(directory),
                expires,
            }),
        ),
        ToolCall::CreateInbox {
            scope,
            name,
            expires,
        } => (
            scope,
            Command::Inbox {
                command: InboxCommand::Create(CreateInboxArgs { name, expires }),
            },
        ),
        ToolCall::RevokeInbox { scope, inbox_id } => (
            scope,
            Command::Inbox {
                command: InboxCommand::Revoke(RevokeInboxArgs { inbox_id }),
            },
        ),
        ToolCall::SetRetention { .. } | ToolCall::ClearRetention { .. } => {
            return mcp_retention_command(call);
        }
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

fn mcp_retention_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        ToolCall::SetRetention {
            scope,
            latest,
            branch,
            path,
        } => (
            scope,
            Command::Retention {
                command: RetentionCommand::Set(SetRetentionArgs {
                    latest: NonZeroU32::new(latest).ok_or_else(|| {
                        BlobyardError::new(
                            ErrorCode::InvalidRequest,
                            "The retention count must be positive.",
                        )
                    })?,
                    branch,
                    path,
                }),
            },
        ),
        ToolCall::ClearRetention { scope } => (
            scope,
            Command::Retention {
                command: RetentionCommand::Clear,
            },
        ),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

fn backend_error(error: &BlobyardError) -> BackendError {
    BackendError::new(error.code().as_str(), error.message())
}

fn finish_mcp(result: std::io::Result<()>) -> Result<crate::CommandResult, BlobyardError> {
    result.map_err(|_error| BlobyardError::from_code(ErrorCode::InternalError))?;
    Ok(crate::CommandResult::local(serde_json::Value::Null, ""))
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
