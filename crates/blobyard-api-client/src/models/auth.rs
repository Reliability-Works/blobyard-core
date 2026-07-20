use crate::{ApiRequest, Endpoint};
use blobyard_core::SecretString;
use serde::{Deserialize, Serialize};

/// Exchanges one-time standalone bootstrap authority.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapExchangeRequest {
    /// Human-readable credential name.
    pub name: String,
    /// Host platform reported by the CLI.
    pub platform: String,
    /// One-time bootstrap capability read from standard input.
    pub token: SecretString,
    /// CLI semantic version.
    pub version: String,
}

impl BootstrapExchangeRequest {
    /// Encodes the exchange body without exposing the bootstrap token.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "platform": self.platform,
            "token": self.token,
            "version": self.version,
        })
    }
}

/// Scoped local operator token returned once after bootstrap exchange.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapExchangeResponse {
    /// Bearer token stored in the selected profile credential slot.
    pub access_token: SecretString,
    /// Granted local operation scopes.
    pub scopes: Vec<String>,
    /// Trusted root origin used by public Web Yard subdomains.
    pub web_yard_origin: String,
    /// Default local workspace namespace.
    pub workspace: String,
}

/// Starts browser-approved CLI authorization.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceStartRequest {
    /// A user-selected label for the CLI session.
    pub name: String,
    /// Host platform identifier.
    pub platform: String,
    /// CLI semantic version.
    pub version: String,
}

impl DeviceStartRequest {
    /// Encodes the strict device-start request without a fallible serializer boundary.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "platform": self.platform,
            "version": self.version,
        })
    }
}

/// Device-flow instructions returned once.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStartResponse {
    /// High-entropy polling credential.
    pub device_code: SecretString,
    /// Ambiguity-free code shown to the user.
    pub user_code: SecretString,
    /// Canonical browser verification URL.
    pub verification_uri: String,
    /// Absolute expiration timestamp.
    pub expires_at: String,
    /// Minimum poll interval in seconds.
    pub poll_interval_seconds: u16,
}

/// Polls an in-progress device authorization.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevicePollRequest {
    /// High-entropy device credential.
    pub device_code: SecretString,
}

impl DevicePollRequest {
    /// Encodes the strict device-poll request without exposing the raw device code in diagnostics.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "deviceCode": self.device_code })
    }
}

/// State of a device authorization poll.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePollState {
    /// Approval has not happened yet.
    Pending,
    /// The server requested slower polling.
    SlowDown,
    /// The user denied the request.
    Denied,
    /// The device request expired.
    Expired,
    /// The request was approved and tokens are present.
    Approved,
}

/// Result of a device authorization poll.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePollResponse {
    /// Current state.
    pub status: DevicePollState,
    /// Token pair, present only after approval.
    #[serde(default)]
    pub tokens: Option<TokenPair>,
}

/// Rotates an existing CLI refresh token.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenRefreshRequest {
    /// Current refresh token.
    pub refresh_token: SecretString,
}

impl TokenRefreshRequest {
    /// Encodes the bounded refresh request for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "refreshToken": self.refresh_token })
    }
}

/// A newly issued opaque CLI token pair.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPair {
    /// Short-lived API access token.
    pub access_token: SecretString,
    /// Rotating refresh token.
    pub refresh_token: SecretString,
    /// Access-token lifetime in seconds.
    pub expires_in_seconds: u16,
}

/// Revokes the current CLI session.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogoutRequest {}

/// The authenticated CLI or CI identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalType {
    /// Browser-approved developer CLI session.
    Cli,
    /// GitHub OIDC-backed machine session.
    Ci,
}

/// The authenticated CLI or CI identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhoAmIResponse {
    /// Resolved authentication principal class.
    pub principal_type: PrincipalType,
    /// Stable user or machine identifier.
    pub principal_id: String,
    /// Safe display label.
    pub display_name: String,
    /// Email address approved through a browser session; absent for CI.
    pub email: Option<String>,
    /// Granted action scopes.
    pub scopes: Vec<String>,
    /// Default workspace selected for CLI operations.
    pub default_workspace: WhoAmIDefaultWorkspace,
}

/// Default workspace returned with the authenticated identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhoAmIDefaultWorkspace {
    /// Stable workspace identifier.
    pub id: String,
    /// Safe workspace display name.
    pub name: String,
    /// Workspace slug used in Blobyard URIs.
    pub slug: String,
}

/// Exchanges a GitHub Actions OIDC assertion.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitHubOidcExchangeRequest {
    /// GitHub-signed OIDC JWT sent only through the authorization header.
    pub assertion: SecretString,
    /// Exact short-lived actions requested by this workflow run.
    pub actions: Vec<String>,
    /// Requested project slug.
    pub project: String,
    /// Requested workspace slug, or the deployment default when omitted.
    pub workspace: Option<String>,
}

impl GitHubOidcExchangeRequest {
    /// Builds the exact exchange request without placing the assertion in JSON.
    #[must_use]
    pub fn into_request(self) -> ApiRequest {
        let mut body = serde_json::json!({
            "actions": self.actions,
            "project": self.project,
        });
        if let Some(workspace) = self.workspace {
            body["workspace"] = serde_json::Value::String(workspace);
        }
        ApiRequest::new(Endpoint::GitHubOidcExchange)
            .with_bearer(self.assertion)
            .with_json(body)
    }
}

/// Short-lived CI bearer issued from verified OIDC claims.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineTokenResponse {
    /// Scoped short-lived bearer token.
    pub access_token: SecretString,
    /// Token lifetime in seconds.
    pub expires_in_seconds: u16,
    /// Effective scopes.
    pub scopes: Vec<String>,
}
