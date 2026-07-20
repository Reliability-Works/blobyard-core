use crate::account_commands::AccountCommand;
use crate::billing_commands::BillingCommand;
use crate::headless_commands::{
    AuditCommand, InvitesCommand, MembersCommand, PreviewsCommand, SessionsCommand, SharesCommand,
    TokensCommand, TrustsCommand, WorkspacesCommand,
};
use clap::{Args, Subcommand, ValueEnum};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::{fmt, fmt::Formatter};

/// Operations supported by the Blobyard CLI contract.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Configure isolated Blob Yard Cloud or self-hosted connections.
    Profiles {
        /// The profile operation.
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    /// Sign in through Blobyard's browser-approved device flow.
    Login(LoginArgs),
    /// Revoke the current CLI session.
    Logout,
    /// Show the current authenticated identity and scope.
    Whoami,
    /// Create project-local Blobyard configuration.
    Init,
    /// List or create workspaces.
    Workspaces {
        /// The workspace operation.
        #[command(subcommand)]
        command: WorkspacesCommand,
    },
    /// Create hosted billing sessions.
    Billing {
        /// The billing operation.
        #[command(subcommand)]
        command: BillingCommand,
    },
    /// Export or delete the authenticated account.
    Account {
        /// The account operation.
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// List or create projects.
    Projects {
        /// The project operation.
        #[command(subcommand)]
        command: ProjectsCommand,
    },
    /// Upload a file or directory.
    Upload(UploadArgs),
    /// Download an immutable or current object version.
    Download(DownloadArgs),
    /// List objects under an optional URI prefix.
    Ls(ListArgs),
    /// Soft-delete a logical object.
    Rm(RemoveArgs),
    /// Create an expiring share for a local path or Blobyard URI.
    Share(ShareArgs),
    /// List or revoke public shares.
    Shares {
        /// The share-management operation.
        #[command(subcommand)]
        command: SharesCommand,
    },
    /// Publish an isolated static directory preview.
    Preview(PreviewArgs),
    /// List or revoke static previews.
    Previews {
        /// The preview-management operation.
        #[command(subcommand)]
        command: PreviewsCommand,
    },
    /// Deploy a static directory to a named public Web Yard.
    Deploy(DeployArgs),
    /// Inspect or manage Web Yards.
    Yard {
        /// The Web Yard operation.
        #[command(subcommand)]
        command: YardCommand,
    },
    /// Create, list, or revoke guest upload inboxes.
    Inbox {
        /// The inbox operation.
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Set, show, or clear a project retention policy.
    Retention {
        /// The retention operation.
        #[command(subcommand)]
        command: RetentionCommand,
    },
    /// Inspect workspace audit history.
    Audit {
        /// The audit operation.
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Inspect or manage workspace members.
    Members {
        /// The member operation.
        #[command(subcommand)]
        command: MembersCommand,
    },
    /// Inspect or manage workspace invitations.
    Invites {
        /// The invitation operation.
        #[command(subcommand)]
        command: InvitesCommand,
    },
    /// Inspect or manage API tokens.
    Tokens {
        /// The API-token operation.
        #[command(subcommand)]
        command: TokensCommand,
    },
    /// Inspect or manage GitHub OIDC trusts.
    Trusts {
        /// The trust operation.
        #[command(subcommand)]
        command: TrustsCommand,
    },
    /// Inspect or revoke browser-approved CLI sessions.
    Sessions {
        /// The session operation.
        #[command(subcommand)]
        command: SessionsCommand,
    },
    /// Generate shell completion output.
    Completion(CompletionArgs),
    /// Serve Blobyard tools to a local AI client.
    Mcp {
        /// MCP server operation.
        #[command(subcommand)]
        command: McpCommand,
    },
}

/// Local connection profile operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ProfilesCommand {
    /// Exchange a one-time bootstrap token and save a self-hosted profile.
    Add(ProfileAddArgs),
}

/// Adds a self-hosted connection profile.
#[derive(Clone, Debug, Args)]
pub struct ProfileAddArgs {
    /// Profile name used by `--profile` and project configuration.
    pub name: String,
    /// Read the one-time bootstrap token from standard input.
    #[arg(long, required = true)]
    pub token_stdin: bool,
}

/// MCP server operations.
#[derive(Clone, Debug, Subcommand)]
pub enum McpCommand {
    /// Serve newline-delimited MCP messages on standard input and output.
    Serve(McpServeArgs),
}

impl McpCommand {
    pub(crate) const fn serve_arguments(&self) -> &McpServeArgs {
        let Self::Serve(arguments) = self;
        arguments
    }
}

/// Arguments for `blobyard mcp serve`.
#[derive(Clone, Debug, Args)]
pub struct McpServeArgs {
    /// Use the local standard input and output transport.
    #[arg(
        long,
        required = true,
        conflicts_with_all = ["json", "quiet", "verbose"]
    )]
    pub stdio: bool,
}

/// Arguments for `blobyard login`.
#[derive(Clone, Debug, Args)]
pub struct LoginArgs {
    /// Name shown for this CLI session.
    #[arg(long, value_name = "DEVICE_NAME")]
    pub name: Option<String>,
    /// Print the activation URL without opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

/// Project operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ProjectsCommand {
    /// List projects visible in the selected workspace.
    List,
    /// Create a project.
    Create(CreateProjectArgs),
}

/// Arguments for `blobyard projects create`.
#[derive(Clone, Debug, Args)]
pub struct CreateProjectArgs {
    /// Human-readable project name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Arguments for `blobyard upload`.
#[derive(Clone, Debug, Args)]
pub struct UploadArgs {
    /// File or directory to upload.
    #[arg(value_name = "PATH")]
    pub source: PathBuf,
    /// Override the destination logical path.
    #[arg(long, value_name = "LOGICAL_PATH")]
    pub path: Option<String>,
    /// Include files excluded by ignore rules.
    #[arg(long)]
    pub include_ignored: bool,
}

/// Arguments for `blobyard download`.
#[derive(Clone, Debug, Args)]
pub struct DownloadArgs {
    /// Blobyard URI to download.
    #[arg(value_name = "BLOBYARD_URI")]
    pub uri: String,
    /// Destination file path.
    #[arg(long, value_name = "PATH", required = true)]
    pub output: PathBuf,
    /// Replace an existing destination file.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for `blobyard ls`.
#[derive(Clone, Debug, Args)]
pub struct ListArgs {
    /// Optional Blobyard URI prefix.
    #[arg(value_name = "BLOBYARD_URI_PREFIX")]
    pub prefix: Option<String>,
    /// Include immutable object versions.
    #[arg(long)]
    pub versions: bool,
}

/// Arguments for `blobyard rm`.
#[derive(Clone, Debug, Args)]
pub struct RemoveArgs {
    /// Blobyard URI to remove.
    #[arg(value_name = "BLOBYARD_URI")]
    pub uri: String,
}

/// Arguments for `blobyard share`.
#[derive(Clone, Debug, Args)]
pub struct ShareArgs {
    /// Local path or Blobyard URI to share.
    #[arg(value_name = "PATH_OR_URI")]
    pub target: String,
    /// Share lifetime, such as `7d`.
    #[arg(long, value_name = "DURATION")]
    pub expires: Option<String>,
    /// Email the share link to this recipient.
    #[arg(long, value_name = "EMAIL")]
    pub notify: Option<String>,
}

/// Arguments for `blobyard preview`.
#[derive(Clone, Debug, Args)]
pub struct PreviewArgs {
    /// Static directory containing `index.html`.
    #[arg(value_name = "DIRECTORY")]
    pub directory: PathBuf,
    /// Preview lifetime, such as `7d`.
    #[arg(long, value_name = "DURATION")]
    pub expires: Option<String>,
}

/// Arguments for `blobyard deploy`.
#[derive(Clone, Debug, Args)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the public CLI contract exposes four independent deploy switches"
)]
pub struct DeployArgs {
    /// Static directory containing `index.html`.
    #[arg(value_name = "DIRECTORY", conflicts_with = "all")]
    pub directory: Option<PathBuf>,
    /// Named Web Yard within the selected project.
    #[arg(long, value_name = "NAME", conflicts_with = "all")]
    pub yard: Option<String>,
    /// Deploy every Web Yard configured in `.blobyard.toml`.
    #[arg(long, conflicts_with_all = ["directory", "yard"])]
    pub all: bool,
    /// Use the root entry file for unmatched extensionless paths.
    #[arg(long)]
    pub spa: bool,
    /// Resolve extensionless paths to matching HTML files.
    #[arg(long)]
    pub clean_urls: bool,
    /// Acknowledge that deployed files become public.
    #[arg(long)]
    pub public: bool,
}

/// Web Yard management operations.
#[derive(Clone, Debug, Subcommand)]
pub enum YardCommand {
    /// List Web Yards in the selected project.
    List,
    /// Show one Web Yard, selecting it automatically when only one exists.
    Show(ShowYardArgs),
    /// List immutable deploy history for a Web Yard.
    History(YardNameArgs),
    /// Repoint a Web Yard to an earlier deploy.
    Rollback(RollbackYardArgs),
    /// Delete a Web Yard and all of its deploys.
    Delete(DeleteYardArgs),
}

/// Arguments for `blobyard yard show`.
#[derive(Clone, Debug, Args)]
pub struct ShowYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Arguments selecting one named Web Yard.
#[derive(Clone, Debug, Args)]
pub struct YardNameArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Arguments for `blobyard yard rollback`.
#[derive(Clone, Debug, Args)]
pub struct RollbackYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Specific immutable deploy identifier. The previous deploy is used when omitted.
    #[arg(value_name = "DEPLOY_ID")]
    pub deploy_id: Option<String>,
}

/// Arguments for `blobyard yard delete`.
#[derive(Clone, Debug, Args)]
pub struct DeleteYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Confirm deletion without an interactive prompt.
    #[arg(long)]
    pub force: bool,
}

/// Inbox operations.
#[derive(Clone, Debug, Subcommand)]
pub enum InboxCommand {
    /// Create a guest upload inbox.
    Create(CreateInboxArgs),
    /// List redacted inbox metadata.
    List,
    /// Revoke an inbox.
    Revoke(RevokeInboxArgs),
}

/// Arguments for `blobyard inbox create`.
#[derive(Clone, Debug, Args)]
pub struct CreateInboxArgs {
    /// Human-readable inbox name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Inbox lifetime, such as `24h`.
    #[arg(long, value_name = "DURATION")]
    pub expires: Option<String>,
}

/// Arguments for `blobyard inbox revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeInboxArgs {
    /// Stable inbox identifier.
    #[arg(value_name = "INBOX_ID")]
    pub inbox_id: String,
}

/// Retention-policy operations.
#[derive(Clone, Debug, Subcommand)]
pub enum RetentionCommand {
    /// Set or replace a retention policy.
    Set(SetRetentionArgs),
    /// Show the selected project's policy.
    Show,
    /// Show policy and the latest retention run.
    Overview,
    /// Clear the selected project's policy.
    Clear,
}

/// Arguments for `blobyard retention set`.
#[derive(Clone, Debug, Args)]
pub struct SetRetentionArgs {
    /// Number of newest matching versions to retain.
    #[arg(long, value_name = "COUNT", required = true)]
    pub latest: NonZeroU32,
    /// Match explicit git branch provenance with this glob.
    #[arg(long, value_name = "GLOB")]
    pub branch: Option<String>,
    /// Match normalized logical paths with this glob.
    #[arg(long, value_name = "GLOB")]
    pub path: Option<String>,
}

/// Arguments for `blobyard completion`.
#[derive(Clone, Debug, Args)]
pub struct CompletionArgs {
    /// Shell whose completion script should be generated.
    #[arg(value_enum, value_name = "SHELL")]
    pub shell: CompletionShell,
}

/// Supported shell completion targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    /// Bash.
    Bash,
    /// Z shell.
    Zsh,
    /// Fish.
    Fish,
}

impl fmt::Display for CompletionShell {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
        })
    }
}
