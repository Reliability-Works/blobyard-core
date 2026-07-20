use crate::{Endpoint, HttpMethod};

impl Endpoint {
    /// Returns the exact versioned endpoint path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        ENDPOINT_PATHS[self as usize]
    }

    /// Returns the endpoint's HTTP method.
    #[must_use]
    pub const fn method(self) -> HttpMethod {
        match self {
            Self::Health
            | Self::WhoAmI
            | Self::ListWorkspaces
            | Self::ListProjects
            | Self::ListObjects
            | Self::ListShares
            | Self::UploadStatus
            | Self::ResolveShare
            | Self::ResolvePreview
            | Self::ListPreviews
            | Self::ListInboxes
            | Self::ResolveInbox
            | Self::GetRetention
            | Self::ListWebYards
            | Self::ListYardDeploys
            | Self::ListAudit
            | Self::ListMembers
            | Self::ListInvites
            | Self::ListApiTokens
            | Self::ListCiTrusts
            | Self::ListCliSessions
            | Self::GetBilling
            | Self::GetAccountExport
            | Self::GetAccountDeletion
            | Self::GetRetentionOverview => HttpMethod::Get,
            Self::DeleteObject | Self::ClearRetention => HttpMethod::Delete,
            Self::SetRetention => HttpMethod::Put,
            _ => HttpMethod::Post,
        }
    }
}

const ENDPOINT_PATHS: [&str; 76] = [
    "/v1/health",
    "/v1/bootstrap/exchange",
    "/v1/cli/device/start",
    "/v1/cli/device/poll",
    "/v1/cli/token/refresh",
    "/v1/cli/logout",
    "/v1/cli/whoami",
    "/v1/workspaces",
    "/v1/workspaces",
    "/v1/projects",
    "/v1/projects",
    "/v1/objects",
    "/v1/objects",
    "/v1/uploads/request",
    "/v1/uploads/parts/request",
    "/v1/uploads/complete",
    "/v1/uploads/abort",
    "/v1/uploads/status",
    "/v1/downloads/request",
    "/v1/shares",
    "/v1/shares",
    "/v1/shares/resolve",
    "/v1/shares/download",
    "/v1/shares/revoke",
    "/v1/previews",
    "/v1/previews",
    "/v1/previews/resolve",
    "/v1/previews/revoke",
    "/v1/inboxes",
    "/v1/inboxes",
    "/v1/inboxes/revoke",
    "/v1/inboxes/resolve",
    "/v1/retention",
    "/v1/retention",
    "/v1/retention",
    "/v1/yards/deploys/start",
    "/v1/yards/deploys/finalise",
    "/v1/yards/deploys/fail",
    "/v1/yards",
    "/v1/yards/deploys",
    "/v1/yards/rollback",
    "/v1/yards/delete",
    "/v1/audit",
    "/v1/members",
    "/v1/members/invites",
    "/v1/members/invites",
    "/v1/members/invites/revoke",
    "/v1/members/role",
    "/v1/members/remove",
    "/v1/api-tokens",
    "/v1/api-tokens",
    "/v1/api-tokens/revoke",
    "/v1/ci/trusts",
    "/v1/ci/trusts",
    "/v1/ci/trusts/revoke",
    "/v1/cli/sessions",
    "/v1/cli/sessions/revoke",
    "/v1/ci/github/oidc/exchange",
    "/v1/secrets",
    "/v1/secrets/redeem",
    "/v1/workspaces/rename",
    "/v1/billing/checkout",
    "/v1/billing/portal",
    "/v1/billing",
    "/v1/billing/storage/checkout",
    "/v1/billing/storage/update",
    "/v1/billing/subscription/update",
    "/v1/account/exports",
    "/v1/account/exports",
    "/v1/account/exports/download",
    "/v1/account/deletion/prepare",
    "/v1/account/deletion/complete",
    "/v1/account/deletion",
    "/v1/account/deletion/retry",
    "/v1/retention/overview",
    "/v1/stripe/webhook",
];
