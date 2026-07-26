use crate::{NewAuditEvent, RepositoryError};

/// Persisted lifecycle state of one local user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalUserStatus {
    /// The user may sign in with an active key.
    Active,
    /// The user is tombstoned and admits nothing.
    Deactivated,
}

impl LocalUserStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deactivated => "deactivated",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "deactivated" => Some(Self::Deactivated),
            _ => None,
        }
    }
}

/// One local user account able to sign in to non-public Yards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalUserRecord {
    /// Stable user identifier.
    pub id: String,
    /// Owning local workspace identifier.
    pub workspace_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional email unique among active users in the workspace.
    pub email: Option<String>,
    /// Persisted lifecycle state.
    pub status: LocalUserStatus,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Deactivation time when the user is tombstoned.
    pub deactivated_at_ms: Option<u64>,
}

/// One hashed local-user sign-in key stored without its raw value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalUserLoginKeyRecord {
    /// Stable key identifier.
    pub id: String,
    /// Owning local user identifier.
    pub user_id: String,
    /// Non-secret prefix shown in listings.
    pub token_prefix: String,
    /// Lowercase SHA-256 digest of the raw high-entropy key.
    pub secret_hash: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Most recent successful authentication time.
    pub last_used_at_ms: Option<u64>,
    /// Revocation time when the key is no longer active.
    pub revoked_at_ms: Option<u64>,
}

/// One local user together with the non-secret prefix of its active sign-in key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalUserListing {
    /// The user account.
    pub user: LocalUserRecord,
    /// Non-secret prefix of the active sign-in key when one exists.
    pub active_key_prefix: Option<String>,
}

/// Durable local-user accounts and their hashed sign-in keys.
pub trait LocalUserRepository: Send + Sync {
    /// Atomically creates one local user, its first sign-in key, and the audit event without
    /// retaining the raw key value.
    ///
    /// # Errors
    ///
    /// Returns conflict for duplicate identifiers, hashes, or an active duplicate email in the
    /// workspace, not-found for an unknown workspace, validation for malformed input, or a
    /// provider failure.
    fn create_local_user(
        &self,
        user: &LocalUserRecord,
        key: &LocalUserLoginKeyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Lists local users in one workspace, newest first, with active key prefixes and never any
    /// digests or raw values.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_local_users(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalUserListing>, RepositoryError>;

    /// Atomically revokes every active sign-in key for one active user, mints the replacement key,
    /// and records the audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown user, conflict for a deactivated user or duplicate key
    /// material, validation for malformed input, or a provider failure.
    fn reset_local_user_login_key(
        &self,
        key: &LocalUserLoginKeyRecord,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Atomically tombstones one active user, revokes every active sign-in key, and records the
    /// audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown user, conflict when already deactivated, validation for a
    /// malformed identifier, or a provider failure.
    fn deactivate_local_user(
        &self,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Resolves the active user behind one active, unexpired sign-in key by its digest and records
    /// successful use.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown, revoked, or expired key or a deactivated user, validation
    /// for a malformed digest, or a provider failure.
    fn authenticate_local_user_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalUserRecord, RepositoryError>;
}

#[cfg(test)]
#[path = "local_users_tests.rs"]
mod tests;
