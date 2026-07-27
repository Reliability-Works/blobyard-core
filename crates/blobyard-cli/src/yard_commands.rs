//! Web Yard and environment command definitions.

use clap::{Args, Subcommand};
use std::path::PathBuf;

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

/// Web Yard environment operations.
#[derive(Clone, Debug, Subcommand)]
pub enum EnvCommand {
    /// List active environments for a Web Yard.
    List(EnvListArgs),
}

/// Arguments for `blobyard env list`.
#[derive(Clone, Debug, Args)]
pub struct EnvListArgs {
    /// Project-unique Web Yard name, selected automatically when only one exists.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Web Yard access-policy operations.
#[derive(Clone, Debug, Subcommand)]
pub enum AccessCommand {
    /// Show effective visibility and active grants for a Web Yard.
    List(AccessListArgs),
    /// Set a Web Yard's visibility.
    SetVisibility(SetVisibilityArgs),
    /// Grant one principal scoped access to a Web Yard.
    Grant(GrantAccessArgs),
    /// Revoke one Web Yard access grant.
    Revoke(RevokeAccessArgs),
    /// Replace one Web Yard access grant's application roles.
    SetRoles(SetAccessRolesArgs),
}

/// Arguments for `blobyard access list`.
#[derive(Clone, Debug, Args)]
pub struct AccessListArgs {
    /// Project-unique Web Yard name, selected automatically when only one exists.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Arguments for `blobyard access set-visibility`.
#[derive(Clone, Debug, Args)]
pub struct SetVisibilityArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Audience: public, owner, selected, workspace, authenticated-link, or any-authenticated.
    #[arg(value_name = "VISIBILITY")]
    pub visibility: String,
}

/// Arguments for `blobyard access grant`.
#[derive(Clone, Debug, Args)]
pub struct GrantAccessArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Principal kind: user, group, or link.
    #[arg(long, value_name = "KIND")]
    pub principal_kind: String,
    /// Stable principal identifier.
    #[arg(long, value_name = "PRINCIPAL_ID")]
    pub principal_id: String,
    /// Application role granted to the principal. Repeatable.
    #[arg(long = "role", value_name = "ROLE")]
    pub roles: Vec<String>,
    /// Restrict the grant to one environment identifier.
    #[arg(long, value_name = "ENVIRONMENT_ID")]
    pub environment: Option<String>,
    /// RFC 3339 expiry timestamp.
    #[arg(long, value_name = "TIMESTAMP")]
    pub expires: Option<String>,
}

/// Arguments for `blobyard access revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeAccessArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable grant identifier.
    #[arg(value_name = "GRANT_ID")]
    pub grant_id: String,
}

/// Arguments for `blobyard access set-roles`.
#[derive(Clone, Debug, Args)]
pub struct SetAccessRolesArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable grant identifier.
    #[arg(value_name = "GRANT_ID")]
    pub grant_id: String,
    /// Replacement application role. Repeatable; omit all roles to clear.
    #[arg(long = "role", value_name = "ROLE")]
    pub roles: Vec<String>,
}

/// Yard guest-invitation operations.
#[derive(Clone, Debug, Subcommand)]
pub enum GuestInvitesCommand {
    /// List retained guest invitations for a Web Yard.
    List(GuestInvitesListArgs),
    /// Create one expiring guest invitation.
    Create(CreateGuestInviteArgs),
    /// Revoke one guest invitation.
    Revoke(RevokeGuestInviteArgs),
}

/// Arguments for `blobyard guest-invites list`.
#[derive(Clone, Debug, Args)]
pub struct GuestInvitesListArgs {
    /// Project-unique Web Yard name, selected automatically when only one exists.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    /// Opaque continuation cursor returned by the previous page.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

/// Arguments for `blobyard guest-invites create`.
#[derive(Clone, Debug, Args)]
pub struct CreateGuestInviteArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Normalized guest email address.
    #[arg(value_name = "EMAIL")]
    pub email: String,
    /// Application role granted to the guest. Repeatable.
    #[arg(long = "role", value_name = "ROLE")]
    pub roles: Vec<String>,
    /// Restrict the invitation to one environment identifier.
    #[arg(long, value_name = "ENVIRONMENT_ID")]
    pub environment: Option<String>,
    /// RFC 3339 expiry timestamp; defaults to seven days.
    #[arg(long, value_name = "TIMESTAMP")]
    pub expires: Option<String>,
}

/// Arguments for `blobyard guest-invites revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeGuestInviteArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable guest-invitation identifier.
    #[arg(value_name = "INVITATION_ID")]
    pub invitation_id: String,
}

/// Yard management-role operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ManagementRolesCommand {
    /// List Yard management-role assignments.
    List(ManagementRolesListArgs),
    /// Create or change one Yard management-role assignment.
    Set(SetManagementRoleArgs),
    /// Revoke one Yard management-role assignment.
    Revoke(RevokeManagementRoleArgs),
}

/// Arguments for `blobyard management-roles list`.
#[derive(Clone, Debug, Args)]
pub struct ManagementRolesListArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Opaque continuation cursor.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

/// Arguments for `blobyard management-roles set`.
#[derive(Clone, Debug, Args)]
pub struct SetManagementRoleArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable active local-user identifier.
    #[arg(value_name = "USER_ID")]
    pub user_id: String,
    /// Management role: owner, admin, developer, or auditor.
    #[arg(value_name = "ROLE")]
    pub role: String,
}

/// Arguments for `blobyard management-roles revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeManagementRoleArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable active local-user identifier.
    #[arg(value_name = "USER_ID")]
    pub user_id: String,
}

/// Yard application-policy operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ApplicationPolicyCommand {
    /// Read the current approved policy.
    Get(ApplicationPolicyGetArgs),
    /// Approve a policy graph from a JSON file.
    Set(ApplicationPolicySetArgs),
}

/// Arguments for `blobyard application-policy get`.
#[derive(Clone, Debug, Args)]
pub struct ApplicationPolicyGetArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Arguments for `blobyard application-policy set`.
#[derive(Clone, Debug, Args)]
pub struct ApplicationPolicySetArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// JSON file containing `defaultRole` and `roles`.
    #[arg(long, value_name = "FILE", required_unless_present = "policy_json")]
    pub policy: Option<PathBuf>,
    /// Inline policy JSON used by MCP and non-interactive callers.
    #[arg(long, value_name = "JSON", conflicts_with = "policy")]
    pub policy_json: Option<String>,
    /// SHA-256 digest of the canonical source manifest.
    #[arg(long, value_name = "DIGEST")]
    pub source_manifest_digest: String,
}

/// Web Yard browser-session operations.
#[derive(Clone, Debug, Subcommand)]
pub enum YardSessionsCommand {
    /// List retained browser sessions for a Web Yard.
    List(YardSessionsListArgs),
    /// Revoke one retained browser session.
    Revoke(RevokeYardSessionArgs),
}

/// Arguments for `blobyard yard-sessions list`.
#[derive(Clone, Debug, Args)]
pub struct YardSessionsListArgs {
    /// Project-unique Web Yard name, selected automatically when only one exists.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Arguments for `blobyard yard-sessions revoke`.
#[derive(Clone, Debug, Args)]
pub struct RevokeYardSessionArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Stable Yard browser-session identifier.
    #[arg(value_name = "SESSION_ID")]
    pub session_id: String,
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
