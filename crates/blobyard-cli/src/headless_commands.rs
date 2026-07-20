use clap::{Args, Subcommand, ValueEnum};

/// Workspace operations.
#[derive(Clone, Debug, Subcommand)]
pub enum WorkspacesCommand {
    /// List workspaces visible to the current identity.
    List,
    /// Create a workspace.
    Create(CreateWorkspaceArgs),
    /// Rename the selected workspace.
    Rename(RenameWorkspaceArgs),
}

/// Arguments for `blobyard workspaces create`.
#[derive(Clone, Debug, Args)]
pub struct CreateWorkspaceArgs {
    /// Human-readable workspace name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Arguments for `blobyard workspaces rename`.
#[derive(Clone, Debug, Args)]
pub struct RenameWorkspaceArgs {
    /// Replacement human-readable workspace name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Share-management operations.
#[derive(Clone, Debug, Subcommand)]
pub enum SharesCommand {
    /// List redacted workspace shares.
    List,
    /// Revoke a share by stable identifier.
    Revoke(RevokeShareArgs),
}

/// Arguments for `blobyard shares revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeShareArgs {
    /// Stable share identifier, never a capability token.
    #[arg(value_name = "SHARE_ID")]
    pub share_id: String,
}

/// Preview-management operations.
#[derive(Clone, Debug, Subcommand)]
pub enum PreviewsCommand {
    /// List redacted previews in the selected project.
    List,
    /// Revoke a preview by stable identifier.
    Revoke(RevokePreviewArgs),
}

/// Arguments for `blobyard previews revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokePreviewArgs {
    /// Stable preview identifier, never a capability token.
    #[arg(value_name = "PREVIEW_ID")]
    pub preview_id: String,
}

/// Audit operations.
#[derive(Clone, Debug, Subcommand)]
pub enum AuditCommand {
    /// List redacted workspace audit events.
    List(CursorArgs),
}

/// Optional cursor accepted by bounded list operations.
#[derive(Clone, Debug, Default, Args)]
pub struct CursorArgs {
    /// Opaque continuation cursor from the previous page.
    #[arg(long)]
    pub cursor: Option<String>,
}

/// Member-management operations.
#[derive(Clone, Debug, Subcommand)]
pub enum MembersCommand {
    /// List workspace members.
    List,
    /// Change a workspace member role.
    Role(MemberRoleArgs),
    /// Remove a workspace member.
    Remove(RemoveMemberArgs),
}

/// Workspace roles accepted by member and invite operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum WorkspaceRole {
    /// Workspace owner.
    Owner,
    /// Workspace administrator.
    Admin,
    /// Workspace member.
    Member,
}

impl WorkspaceRole {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

/// Arguments for `blobyard members role`.
#[derive(Clone, Debug, Args)]
pub struct MemberRoleArgs {
    /// Stable user identifier.
    #[arg(value_name = "USER_ID")]
    pub user_id: String,
    /// New workspace role.
    #[arg(long, value_enum)]
    pub role: WorkspaceRole,
}

/// Arguments for `blobyard members remove`.
#[derive(Clone, Debug, Args)]
pub struct RemoveMemberArgs {
    /// Stable user identifier.
    #[arg(value_name = "USER_ID")]
    pub user_id: String,
    /// Confirm removal without an interactive prompt.
    #[arg(long)]
    pub force: bool,
}

/// Workspace invitation operations.
#[derive(Clone, Debug, Subcommand)]
pub enum InvitesCommand {
    /// List redacted workspace invitations.
    List,
    /// Create a workspace invitation.
    Create(CreateInviteArgs),
    /// Revoke a workspace invitation.
    Revoke(RevokeInviteArgs),
}

/// Arguments for `blobyard invites create`.
#[derive(Clone, Debug, Args)]
pub struct CreateInviteArgs {
    /// Recipient email address.
    #[arg(value_name = "EMAIL")]
    pub email: String,
    /// Initial workspace role.
    #[arg(long, value_enum, default_value = "member")]
    pub role: WorkspaceRole,
}

/// Arguments for `blobyard invites revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeInviteArgs {
    /// Stable invitation identifier.
    #[arg(value_name = "INVITE_ID")]
    pub invite_id: String,
}

/// API-token operations.
#[derive(Clone, Debug, Subcommand)]
pub enum TokensCommand {
    /// List redacted API-token metadata.
    List,
    /// Create a scoped API token and show it once.
    Create(CreateTokenArgs),
    /// Revoke an API token.
    Revoke(RevokeTokenArgs),
}

/// Arguments for `blobyard tokens create`.
#[derive(Clone, Debug, Args)]
pub struct CreateTokenArgs {
    /// Human-readable token name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Lifetime in whole days.
    #[arg(long, value_name = "DAYS")]
    pub expires_days: u16,
    /// One granted API scope. Repeat for multiple scopes.
    #[arg(long = "scope", value_name = "SCOPE", required = true)]
    pub scopes: Vec<String>,
}

/// Arguments for `blobyard tokens revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeTokenArgs {
    /// Stable API-token identifier.
    #[arg(value_name = "TOKEN_ID")]
    pub token_id: String,
}

/// GitHub OIDC trust operations.
#[derive(Clone, Debug, Subcommand)]
pub enum TrustsCommand {
    /// List redacted GitHub OIDC trusts.
    List,
    /// Create a scoped GitHub OIDC trust.
    Create(CreateTrustArgs),
    /// Revoke a GitHub OIDC trust.
    Revoke(RevokeTrustArgs),
}

/// Arguments for `blobyard trusts create`.
#[derive(Clone, Debug, Args)]
pub struct CreateTrustArgs {
    /// GitHub owner/repository.
    #[arg(long)]
    pub repository: String,
    /// Workflow path under `.github/workflows`.
    #[arg(long)]
    pub workflow_path: String,
    /// Pinned workflow git ref.
    #[arg(long)]
    pub workflow_ref: String,
    /// Allowed Git ref glob.
    #[arg(long)]
    pub allowed_ref_glob: String,
    /// Allowed action. Repeat for upload, download, or share.
    #[arg(long = "action", required = true)]
    pub allowed_actions: Vec<String>,
    /// Optional GitHub environment.
    #[arg(long)]
    pub environment: Option<String>,
}

/// Arguments for `blobyard trusts revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeTrustArgs {
    /// Stable trust identifier.
    #[arg(value_name = "TRUST_ID")]
    pub trust_id: String,
}

/// CLI-session operations.
#[derive(Clone, Debug, Subcommand)]
pub enum SessionsCommand {
    /// List active browser-approved sessions.
    List,
    /// Revoke a CLI session.
    Revoke(RevokeSessionArgs),
}

/// Arguments for `blobyard sessions revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeSessionArgs {
    /// Stable CLI-session identifier.
    #[arg(value_name = "SESSION_ID")]
    pub session_id: String,
}
