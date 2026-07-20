use clap::{Args, Subcommand};

/// Account lifecycle operations.
#[derive(Clone, Debug, Subcommand)]
pub enum AccountCommand {
    /// Export account data.
    Export {
        /// Account export operation.
        #[command(subcommand)]
        command: AccountExportCommand,
    },
    /// Inspect or delete the authenticated account.
    Delete {
        /// Account deletion phase.
        #[command(subcommand)]
        command: AccountDeleteCommand,
    },
}

/// Account export operations.
#[derive(Clone, Debug, Subcommand)]
pub enum AccountExportCommand {
    /// Queue a portable account data export.
    Request,
    /// Show the current account export state.
    Show,
    /// Issue a short-lived download for one export part.
    Download(AccountExportDownloadArgs),
}

/// Explicit phases for account deletion.
#[derive(Clone, Debug, Subcommand)]
pub enum AccountDeleteCommand {
    /// Show the current deletion state.
    Show,
    /// Issue a short-lived single-use confirmation.
    Prepare,
    /// Consume a confirmation and queue deletion.
    Complete(CompleteAccountDeletionArgs),
    /// Retry a failed deletion job.
    Retry(RetryAccountDeletionArgs),
}

/// Arguments for `blobyard account export download`.
#[derive(Clone, Debug, Args)]
pub struct AccountExportDownloadArgs {
    /// Stable export identifier from the export status.
    #[arg(value_name = "EXPORT_ID")]
    pub export_id: String,
    /// Zero-based export part number.
    #[arg(long, default_value_t = 0, value_name = "NUMBER")]
    pub part_number: u32,
}

/// Arguments for `blobyard account delete complete`.
#[derive(Clone, Debug, Args)]
pub struct CompleteAccountDeletionArgs {
    /// Confirmation returned by `account delete prepare`.
    #[arg(value_name = "CONFIRMATION_TOKEN")]
    pub confirmation_token: String,
    /// Confirm destructive account deletion without an interactive prompt.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for `blobyard account delete retry`.
#[derive(Clone, Debug, Args)]
pub struct RetryAccountDeletionArgs {
    /// Confirm retrying a failed destructive account deletion.
    #[arg(long)]
    pub force: bool,
}
