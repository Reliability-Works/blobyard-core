/// HTTP methods used by Blobyard's API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    /// Read a resource.
    Get,
    /// Create or invoke a resource action.
    Post,
    /// Replace a resource configuration.
    Put,
    /// Delete or revoke a resource.
    Delete,
}

impl HttpMethod {
    /// Returns whether the method is intrinsically safe to retry.
    #[must_use]
    pub const fn is_safe(self) -> bool {
        matches!(self, Self::Get)
    }

    /// Returns the uppercase HTTP token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// Every versioned endpoint in the Blobyard API contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Endpoint {
    /// Service readiness.
    Health,
    /// Exchange one-time standalone bootstrap authority.
    ExchangeBootstrapToken,
    /// Start CLI device authorization.
    DeviceStart,
    /// Poll CLI device authorization.
    DevicePoll,
    /// Rotate a CLI token pair.
    TokenRefresh,
    /// Revoke a CLI session.
    Logout,
    /// Resolve CLI identity and scopes.
    WhoAmI,
    /// List workspaces.
    ListWorkspaces,
    /// Create a workspace.
    CreateWorkspace,
    /// List projects.
    ListProjects,
    /// Create a project.
    CreateProject,
    /// List objects.
    ListObjects,
    /// Soft-delete an object.
    DeleteObject,
    /// Reserve an upload.
    RequestUpload,
    /// Request multipart URLs.
    RequestUploadParts,
    /// Complete an upload.
    CompleteUpload,
    /// Abort an upload.
    AbortUpload,
    /// Read upload status.
    UploadStatus,
    /// Request a signed download.
    RequestDownload,
    /// Create a share.
    CreateShare,
    /// List redacted shares.
    ListShares,
    /// Resolve public share metadata.
    ResolveShare,
    /// Issue a public share download.
    DownloadShare,
    /// Revoke a share.
    RevokeShare,
    /// Create a static preview.
    CreatePreview,
    /// List redacted previews.
    ListPreviews,
    /// Resolve a preview path.
    ResolvePreview,
    /// Revoke a preview.
    RevokePreview,
    /// Create an upload inbox.
    CreateInbox,
    /// List upload inboxes.
    ListInboxes,
    /// Revoke an upload inbox.
    RevokeInbox,
    /// Resolve public inbox metadata.
    ResolveInbox,
    /// Read a retention policy.
    GetRetention,
    /// Set a retention policy.
    SetRetention,
    /// Clear a retention policy.
    ClearRetention,
    /// Start an immutable Web Yard deploy.
    StartYardDeploy,
    /// Finalise an uploaded Web Yard deploy.
    FinaliseYardDeploy,
    /// Mark an interrupted Web Yard deploy as failed.
    FailYardDeploy,
    /// List Web Yards in a project.
    ListWebYards,
    /// List immutable deploys for a Web Yard.
    ListYardDeploys,
    /// List active environments for a Web Yard.
    ListYardEnvironments,
    /// Read a Web Yard's effective visibility and active grants.
    GetYardAccess,
    /// Set a Web Yard's visibility policy.
    SetYardVisibility,
    /// Grant one principal scoped access to a Web Yard.
    GrantYardAccess,
    /// Revoke one Web Yard access grant.
    RevokeYardAccess,
    /// List Yard guest invitations.
    ListYardGuestInvites,
    /// Create one Yard guest invitation.
    CreateYardGuestInvite,
    /// Revoke one Yard guest invitation.
    RevokeYardGuestInvite,
    /// List Yard management-role assignments.
    ListYardManagementRoles,
    /// Create or change one Yard management-role assignment.
    SetYardManagementRole,
    /// Revoke one Yard management-role assignment.
    RevokeYardManagementRole,
    /// Read one Yard's approved application policy.
    GetYardApplicationPolicy,
    /// Approve one Yard application policy.
    SetYardApplicationPolicy,
    /// Replace one Yard access grant's application roles.
    SetYardAccessRoles,
    /// List retained browser sessions for one Web Yard.
    ListYardSessions,
    /// Revoke one Web Yard browser session.
    RevokeYardSession,
    /// Repoint a Web Yard to an earlier deploy.
    RollbackWebYard,
    /// Delete a Web Yard and schedule its objects for cleanup.
    DeleteWebYard,
    /// List workspace audit events.
    ListAudit,
    /// List workspace members.
    ListMembers,
    /// List workspace invitations.
    ListInvites,
    /// Create a workspace invitation.
    CreateInvite,
    /// Revoke a workspace invitation.
    RevokeInvite,
    /// Change a workspace member role.
    UpdateMemberRole,
    /// Remove a workspace member.
    RemoveMember,
    /// List user API tokens.
    ListApiTokens,
    /// Create a user API token.
    CreateApiToken,
    /// Revoke a user API token.
    RevokeApiToken,
    /// List workspace groups.
    ListGroups,
    /// Create a workspace group.
    CreateGroup,
    /// Rename a workspace group.
    RenameGroup,
    /// List current group members.
    ListGroupMembers,
    /// Add one group member.
    AddGroupMember,
    /// Remove one group member.
    RemoveGroupMember,
    /// Deactivate one workspace group.
    DeactivateGroup,
    /// List local users with active sign-in key prefixes.
    ListLocalUsers,
    /// Create a local user and return the raw sign-in key once.
    CreateLocalUser,
    /// Replace a local user's sign-in key and return the raw value once.
    ResetLocalUserLoginKey,
    /// Deactivate a local user and revoke every active sign-in key.
    DeactivateLocalUser,
    /// List GitHub OIDC trusts.
    ListCiTrusts,
    /// Create a GitHub OIDC trust.
    CreateCiTrust,
    /// Revoke a GitHub OIDC trust.
    RevokeCiTrust,
    /// List active CLI sessions.
    ListCliSessions,
    /// Revoke an active CLI session.
    RevokeCliSession,
    /// Exchange a GitHub OIDC assertion.
    GitHubOidcExchange,
    /// Create a client-encrypted one-time secret.
    CreateOneTimeSecret,
    /// Redeem a client-encrypted one-time secret.
    RedeemOneTimeSecret,
    /// Rename a workspace through bearer authentication.
    RenameWorkspace,
    /// Create a hosted billing checkout session.
    CreateBillingCheckout,
    /// Create a hosted billing management portal session.
    CreateBillingPortal,
    /// Read the current billing projection.
    GetBilling,
    /// Create hosted checkout for managed storage.
    CreateStorageCheckout,
    /// Update managed storage through hosted billing.
    CreateStorageUpdate,
    /// Update the paid plan or Team seat count.
    CreateBillingSubscriptionUpdate,
    /// Queue an account data export.
    RequestAccountExport,
    /// Read the current account export state.
    GetAccountExport,
    /// Issue a short-lived account export download.
    DownloadAccountExport,
    /// Issue a short-lived confirmation and a deletion recovery capability.
    PrepareAccountDeletion,
    /// Consume a confirmation and queue account deletion.
    CompleteAccountDeletion,
    /// Read the current account deletion state.
    GetAccountDeletion,
    /// Retry a failed account deletion job.
    RetryAccountDeletion,
    /// Read retention policy and last-run status.
    GetRetentionOverview,
    /// Receive a Stripe webhook.
    StripeWebhook,
}

impl Endpoint {
    /// All endpoints, for contract validation.
    pub const ALL: [Self; 103] = [
        Self::Health,
        Self::ExchangeBootstrapToken,
        Self::DeviceStart,
        Self::DevicePoll,
        Self::TokenRefresh,
        Self::Logout,
        Self::WhoAmI,
        Self::ListWorkspaces,
        Self::CreateWorkspace,
        Self::ListProjects,
        Self::CreateProject,
        Self::ListObjects,
        Self::DeleteObject,
        Self::RequestUpload,
        Self::RequestUploadParts,
        Self::CompleteUpload,
        Self::AbortUpload,
        Self::UploadStatus,
        Self::RequestDownload,
        Self::CreateShare,
        Self::ListShares,
        Self::ResolveShare,
        Self::DownloadShare,
        Self::RevokeShare,
        Self::CreatePreview,
        Self::ListPreviews,
        Self::ResolvePreview,
        Self::RevokePreview,
        Self::CreateInbox,
        Self::ListInboxes,
        Self::RevokeInbox,
        Self::ResolveInbox,
        Self::GetRetention,
        Self::SetRetention,
        Self::ClearRetention,
        Self::StartYardDeploy,
        Self::FinaliseYardDeploy,
        Self::FailYardDeploy,
        Self::ListWebYards,
        Self::ListYardDeploys,
        Self::ListYardEnvironments,
        Self::GetYardAccess,
        Self::SetYardVisibility,
        Self::GrantYardAccess,
        Self::RevokeYardAccess,
        Self::ListYardGuestInvites,
        Self::CreateYardGuestInvite,
        Self::RevokeYardGuestInvite,
        Self::ListYardManagementRoles,
        Self::SetYardManagementRole,
        Self::RevokeYardManagementRole,
        Self::GetYardApplicationPolicy,
        Self::SetYardApplicationPolicy,
        Self::SetYardAccessRoles,
        Self::ListYardSessions,
        Self::RevokeYardSession,
        Self::RollbackWebYard,
        Self::DeleteWebYard,
        Self::ListAudit,
        Self::ListMembers,
        Self::ListInvites,
        Self::CreateInvite,
        Self::RevokeInvite,
        Self::UpdateMemberRole,
        Self::RemoveMember,
        Self::ListApiTokens,
        Self::CreateApiToken,
        Self::RevokeApiToken,
        Self::ListGroups,
        Self::CreateGroup,
        Self::RenameGroup,
        Self::ListGroupMembers,
        Self::AddGroupMember,
        Self::RemoveGroupMember,
        Self::DeactivateGroup,
        Self::ListLocalUsers,
        Self::CreateLocalUser,
        Self::ResetLocalUserLoginKey,
        Self::DeactivateLocalUser,
        Self::ListCiTrusts,
        Self::CreateCiTrust,
        Self::RevokeCiTrust,
        Self::ListCliSessions,
        Self::RevokeCliSession,
        Self::GitHubOidcExchange,
        Self::CreateOneTimeSecret,
        Self::RedeemOneTimeSecret,
        Self::RenameWorkspace,
        Self::CreateBillingCheckout,
        Self::CreateBillingPortal,
        Self::GetBilling,
        Self::CreateStorageCheckout,
        Self::CreateStorageUpdate,
        Self::CreateBillingSubscriptionUpdate,
        Self::RequestAccountExport,
        Self::GetAccountExport,
        Self::DownloadAccountExport,
        Self::PrepareAccountDeletion,
        Self::CompleteAccountDeletion,
        Self::GetAccountDeletion,
        Self::RetryAccountDeletion,
        Self::GetRetentionOverview,
        Self::StripeWebhook,
    ];

    /// Customer-facing endpoints in the canonical `OpenAPI` document.
    pub const PUBLIC: [Self; 101] = public_endpoints();

    /// Returns whether the endpoint durably replays an `Idempotency-Key` request.
    #[must_use]
    pub const fn supports_idempotency(self) -> bool {
        matches!(
            self,
            Self::RequestUpload
                | Self::CreateBillingCheckout
                | Self::CreateBillingPortal
                | Self::RequestAccountExport
                | Self::PrepareAccountDeletion
                | Self::CompleteAccountDeletion
        )
    }
}

const fn public_endpoints() -> [Endpoint; 101] {
    let mut public = [Endpoint::Health; 101];
    let mut source_index = 0;
    let mut public_index = 0;
    while source_index < Endpoint::ALL.len() {
        let endpoint = Endpoint::ALL[source_index];
        if !matches!(endpoint, Endpoint::ResolvePreview | Endpoint::StripeWebhook) {
            public[public_index] = endpoint;
            public_index += 1;
        }
        source_index += 1;
    }
    public
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
