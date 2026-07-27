use crate::{NewAuditEvent, RepositoryError};

/// Absolute lifetime of one signed Yard login continuation.
pub const YARD_CONTINUATION_LIFETIME_MS: u64 = 600_000;
/// Absolute lifetime of one single-use Yard exchange code.
pub const YARD_EXCHANGE_CODE_LIFETIME_MS: u64 = 60_000;
/// Absolute lifetime of one Yard browser session.
pub const YARD_SESSION_LIFETIME_MS: u64 = 43_200_000;
/// Maximum propagation delay for session or policy revocation in Core.
pub const YARD_SESSION_REVOCATION_BOUND_MS: u64 = 0;
/// Fixed host-only cookie name used by Yard origins.
pub const YARD_SESSION_COOKIE_NAME: &str = "__Host-blobyard-yard-session";
/// Fixed login rate-limit window.
pub const YARD_LOGIN_RATE_WINDOW_MS: u64 = 60_000;
/// Maximum login submissions per fingerprint and fixed window.
pub const YARD_LOGIN_RATE_LIMIT: u32 = 10;

/// One Yard and production environment that admit an active local user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardAdmission {
    /// Admitting Yard identifier.
    pub yard_id: String,
    /// Admitting production environment identifier.
    pub environment_id: String,
    /// Yard workspace used for audit isolation.
    pub workspace_id: String,
}

/// Durable single-use exchange code created after successful local-user login.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardContinuation {
    /// Stable continuation redemption identifier.
    pub id: String,
    /// Lowercase SHA-256 digest of the signed continuation.
    pub continuation_hash: String,
    /// Lowercase SHA-256 digest of the raw exchange code.
    pub code_hash: String,
    /// Admitting Yard identifier.
    pub yard_id: String,
    /// Admitting production environment identifier.
    pub environment_id: String,
    /// Exact Yard host label that may exchange the code.
    pub host_label: String,
    /// Authenticated opaque platform subject identifier.
    pub user_id: String,
    /// Normalized path restored after exchange.
    pub return_path: String,
    /// Code creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute code expiry as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Persisted continuation redemption state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardContinuationRecord {
    /// Durable values supplied when the code was issued.
    pub continuation: NewYardContinuation,
    /// Atomic exchange time when consumed.
    pub consumed_at_ms: Option<u64>,
}

/// Server-generated secret material needed to mint one durable Yard session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardSession {
    /// Stable session identifier.
    pub id: String,
    /// Lowercase SHA-256 digest of the raw session token.
    pub token_hash: String,
    /// Session creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute session expiry as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Non-secret audit identifiers supplied at the atomic exchange boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardSessionAuditContext {
    /// Stable audit event identifier.
    pub id: String,
    /// Correlation identifier for the browser exchange.
    pub request_id: String,
}

/// Persisted Yard browser session bound to one user, environment, and exact host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardSessionRecord {
    /// Stable session identifier.
    pub id: String,
    /// Lowercase SHA-256 digest of the raw session token.
    pub token_hash: String,
    /// Bound Yard identifier.
    pub yard_id: String,
    /// Bound production environment identifier.
    pub environment_id: String,
    /// Exact bound Yard host label.
    pub host_label: String,
    /// Authenticated opaque platform subject identifier.
    pub user_id: String,
    /// Session creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute session expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Most recent admitted private delivery time.
    pub last_used_at_ms: Option<u64>,
    /// Explicit revocation time.
    pub revoked_at_ms: Option<u64>,
}

/// Effective lifecycle state exposed by session management.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardSessionStatus {
    /// The session is unexpired and has not been revoked.
    Active,
    /// The absolute lifetime has elapsed.
    Expired,
    /// The session was explicitly revoked.
    Revoked,
}

impl YardSessionStatus {
    /// Returns the stable API representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }
}

impl YardSessionRecord {
    /// Derives the effective lifecycle state at one instant.
    #[must_use]
    pub const fn status_at(&self, now_ms: u64) -> YardSessionStatus {
        if self.revoked_at_ms.is_some() {
            YardSessionStatus::Revoked
        } else if self.expires_at_ms <= now_ms {
            YardSessionStatus::Expired
        } else {
            YardSessionStatus::Active
        }
    }
}

/// One listed session with its current non-secret local-user display label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardSessionListing {
    /// Durable session metadata.
    pub session: YardSessionRecord,
    /// Current local-user display name.
    pub user_display_name: String,
}

/// Successful atomic code exchange result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardSessionExchange {
    /// Newly persisted session.
    pub session: YardSessionRecord,
    /// Revalidated path restored by the Yard-origin redirect.
    pub return_path: String,
}

/// Durable Yard continuation and browser-session operations.
pub trait YardSessionRepository: Send + Sync {
    /// Resolves whether one active local user may enter the production environment for an exact
    /// active Yard host.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown or denied host/user pair, validation for malformed input,
    /// or a provider failure.
    fn evaluate_yard_admission(
        &self,
        host_label: &str,
        user_id: &str,
        now_ms: u64,
    ) -> Result<YardAdmission, RepositoryError>;

    /// Stores one hashed exchange code and atomically consumes the signed continuation identity.
    ///
    /// # Errors
    ///
    /// Returns conflict for replay or duplicate material, not-found for foreign durable bindings,
    /// validation for malformed input, or a provider failure.
    fn issue_yard_exchange_code(
        &self,
        continuation: &NewYardContinuation,
    ) -> Result<(), RepositoryError>;

    /// Atomically consumes one live host-bound code, mints a hashed session, and records exactly
    /// one `yard.session_issued` audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown, replayed, expired, or wrong-host code, conflict for
    /// duplicate session material, validation for malformed input, or a provider failure.
    fn exchange_yard_session_code(
        &self,
        code_hash: &str,
        host_label: &str,
        session: &NewYardSession,
        audit: &YardSessionAuditContext,
        now_ms: u64,
    ) -> Result<YardSessionExchange, RepositoryError>;

    /// Lists all retained sessions for one Yard, newest first, without raw token material.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_yard_sessions(&self, yard_id: &str)
    -> Result<Vec<YardSessionListing>, RepositoryError>;

    /// Atomically revokes one Yard session and records the management audit event once.
    ///
    /// Returns whether this call newly revoked the session.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown or foreign session, validation for malformed input, or a
    /// provider failure.
    fn revoke_yard_session(
        &self,
        yard_id: &str,
        session_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;

    /// Revokes the active session matching one hashed cookie on its exact host.
    ///
    /// Returns whether one live session was newly revoked.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn revoke_yard_session_by_token(
        &self,
        token_hash: &str,
        host_label: &str,
        now_ms: u64,
    ) -> Result<bool, RepositoryError>;

    /// Purges expired continuation history older than 24 hours and expired or revoked session
    /// history older than 30 days.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures. Authorization never depends on this purge.
    fn purge_yard_session_history(&self, now_ms: u64) -> Result<(), RepositoryError>;
}

#[cfg(test)]
#[path = "yard_sessions_tests.rs"]
mod tests;
