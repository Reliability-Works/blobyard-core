use super::encoding;
use serde::{Deserialize, Serialize};

/// Effective lifecycle state of one Yard browser session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum YardSessionStatus {
    /// The session is unexpired and has not been revoked.
    Active,
    /// The session lifetime has elapsed.
    Expired,
    /// An operator or the user explicitly revoked the session.
    Revoked,
}

/// Stable non-secret metadata for one Yard browser session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardSessionSummary {
    /// Session creation timestamp in RFC 3339 form.
    pub created_at: String,
    /// Bound production environment identifier.
    pub environment_id: String,
    /// Absolute session expiry in RFC 3339 form.
    pub expires_at: String,
    /// Exact bound Yard host label.
    pub host_label: String,
    /// Stable session identifier.
    pub id: String,
    /// Most recent admitted private delivery time in RFC 3339 form.
    pub last_used_at: Option<String>,
    /// Effective session lifecycle state.
    pub status: YardSessionStatus,
    /// Current local-user display name.
    pub user_display_name: String,
    /// Bound local-user identifier.
    pub user_id: String,
    /// Bound Yard identifier.
    pub yard_id: String,
}

/// Selects the Yard whose retained browser sessions are listed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardSessionsQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl ListYardSessionsQuery {
    /// Encodes the session-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id))])
    }
}

/// Retained browser sessions for one Yard.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListYardSessionsResponse {
    /// Sessions ordered newest first.
    pub sessions: Vec<YardSessionSummary>,
}

/// Revokes one retained Yard browser session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeYardSessionRequest {
    /// Stable session identifier.
    pub session_id: String,
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl RevokeYardSessionRequest {
    /// Encodes the session-revocation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "sessionId": self.session_id,
            "yardId": self.yard_id,
        })
    }
}
