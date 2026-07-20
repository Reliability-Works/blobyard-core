use crate::Command;
use crate::account_commands::{AccountCommand, AccountDeleteCommand, AccountExportCommand};
use crate::billing_commands::BillingCommand;
use crate::commands::RetentionCommand;
use crate::headless_commands::{RenameWorkspaceArgs, WorkspacesCommand};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{DashboardToolCall, Scope, ToolCall};

pub(super) fn mcp_dashboard_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        ToolCall::Dashboard(DashboardToolCall::RenameWorkspace { scope, name }) => (
            scope,
            Command::Workspaces {
                command: WorkspacesCommand::Rename(RenameWorkspaceArgs { name }),
            },
        ),
        ToolCall::Dashboard(DashboardToolCall::RequestAccountExport { scope }) => (
            scope,
            Command::Account {
                command: AccountCommand::Export {
                    command: AccountExportCommand::Request,
                },
            },
        ),
        ToolCall::Dashboard(DashboardToolCall::GetBilling { scope }) => (
            scope,
            Command::Billing {
                command: BillingCommand::Show,
            },
        ),
        ToolCall::Dashboard(DashboardToolCall::GetAccountExport { scope }) => (
            scope,
            Command::Account {
                command: AccountCommand::Export {
                    command: AccountExportCommand::Show,
                },
            },
        ),
        ToolCall::Dashboard(DashboardToolCall::GetAccountDeletion { scope }) => (
            scope,
            Command::Account {
                command: AccountCommand::Delete {
                    command: AccountDeleteCommand::Show,
                },
            },
        ),
        ToolCall::Dashboard(DashboardToolCall::GetRetentionOverview { scope }) => (
            scope,
            Command::Retention {
                command: RetentionCommand::Overview,
            },
        ),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}
