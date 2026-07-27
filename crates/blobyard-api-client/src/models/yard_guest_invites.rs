use super::encoding;
use serde::{Deserialize, Serialize};

/// Persisted guest-invitation lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum YardGuestInviteStatus {
    /// The invitation token may be accepted once.
    Pending,
    /// The invitation is bound to one guest subject.
    Accepted,
    /// The invitation and its authority are revoked.
    Revoked,
}

/// Non-secret management projection of one Yard guest invitation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YardGuestInvite {
    /// Acceptance timestamp in RFC 3339 form.
    pub accepted_at: Option<String>,
    /// Application roles granted to the guest.
    pub app_roles: Vec<String>,
    /// Creation timestamp in RFC 3339 form.
    pub created_at: String,
    /// Normalized invited email.
    pub email: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Absolute expiry in RFC 3339 form.
    pub expires_at: String,
    /// Stable invitation identifier.
    pub id: String,
    /// Revocation timestamp in RFC 3339 form.
    pub revoked_at: Option<String>,
    /// Current lifecycle state.
    pub status: YardGuestInviteStatus,
    /// Governed Yard identifier.
    pub yard_id: String,
}

/// Lists one bounded deterministic guest-invitation page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardGuestInvitesQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Opaque next-page cursor.
    pub cursor: Option<String>,
    /// Requested page size, defaulting to 50 when omitted.
    pub limit: Option<u8>,
}

impl ListYardGuestInvitesQuery {
    /// Encodes the invitation-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[
            ("cursor", self.cursor),
            ("limit", self.limit.map(|value| value.to_string())),
            ("yardId", Some(self.yard_id)),
        ])
    }
}

/// One guest-invitation management page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardGuestInvitesResponse {
    /// Ordered invitation records.
    pub items: Vec<YardGuestInvite>,
    /// Opaque next-page cursor.
    pub next_cursor: Option<String>,
}

/// Creates one scoped guest invitation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateYardGuestInviteRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Normalized invited email.
    pub email: String,
    /// Application roles to grant.
    pub app_roles: Vec<String>,
    /// Optional absolute expiry in RFC 3339 form; omitted defaults to seven days.
    pub expires_at: Option<String>,
}

impl CreateYardGuestInviteRequest {
    /// Encodes the invitation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "appRoles": self.app_roles,
            "email": self.email,
            "environmentId": self.environment_id,
            "yardId": self.yard_id,
        });
        if let Some(expires_at) = self.expires_at {
            body["expiresAt"] = serde_json::Value::String(expires_at);
        }
        body
    }
}

/// Newly created invitation and its one-time URL.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateYardGuestInviteResponse {
    /// Non-secret invitation metadata.
    pub invitation: YardGuestInvite,
    /// Raw invitation URL returned once.
    pub invitation_url: String,
}

/// Revokes one guest invitation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeYardGuestInviteRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Stable invitation identifier.
    pub invitation_id: String,
}

impl RevokeYardGuestInviteRequest {
    /// Encodes the revocation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "invitationId": self.invitation_id,
            "yardId": self.yard_id,
        })
    }
}

/// Revoked guest invitation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeYardGuestInviteResponse {
    /// Updated non-secret invitation metadata.
    pub invitation: YardGuestInvite,
}
