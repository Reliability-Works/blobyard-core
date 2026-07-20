use crate::{NewAuditEvent, RepositoryError};

/// One active local API credential stored without its raw bearer token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalApiTokenRecord {
    /// Stable credential identifier.
    pub id: String,
    /// Human-readable operator label.
    pub name: String,
    /// Non-secret prefix shown in token listings.
    pub token_prefix: String,
    /// Lowercase SHA-256 digest of the raw high-entropy token.
    pub secret_hash: String,
    /// Granted operation scopes.
    pub scopes: Vec<String>,
    /// Default local workspace identifier.
    pub workspace_id: String,
    /// Optional project boundary for cleanup credentials.
    pub project_id: Option<String>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Most recent successful authentication time.
    pub last_used_at_ms: Option<u64>,
    /// Revocation time when the credential is no longer active.
    pub revoked_at_ms: Option<u64>,
}

/// One active standalone CLI session backed by a hashed local bearer credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCliSessionRecord {
    /// Stable public session identifier.
    pub id: String,
    /// Internal API-token identifier whose row stores the credential digest.
    pub token_id: String,
    /// Owning local workspace identifier.
    pub workspace_id: String,
    /// Human-readable CLI session label.
    pub name: String,
    /// Host platform reported by the CLI.
    pub platform: String,
    /// CLI semantic version.
    pub version: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Most recent successful bearer authentication.
    pub last_used_at_ms: Option<u64>,
    /// Revocation time when the session is no longer active.
    pub revoked_at_ms: Option<u64>,
}

/// Durable one-time bootstrap and scoped local-token operations.
pub trait CredentialRepository: Send + Sync {
    /// Installs bootstrap authority only for a never-initialized repository.
    ///
    /// Returns `true` when this call installed the authority. Once exchanged, bootstrap authority
    /// remains permanently consumed and cannot be recreated by restarting the service.
    ///
    /// # Errors
    ///
    /// Returns a validation or provider failure.
    fn install_bootstrap(&self, secret_hash: &str) -> Result<bool, RepositoryError>;

    /// Atomically consumes matching bootstrap authority and creates one scoped API token.
    ///
    /// # Errors
    ///
    /// Returns not-found for invalid or consumed bootstrap authority, conflict for a duplicate API
    /// token, validation for malformed input, or a provider failure.
    fn exchange_bootstrap(
        &self,
        bootstrap_hash: &str,
        token: &LocalApiTokenRecord,
        session: &LocalCliSessionRecord,
    ) -> Result<(), RepositoryError>;

    /// Lists active CLI sessions in one workspace without credential digests or raw tokens.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_cli_sessions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCliSessionRecord>, RepositoryError>;

    /// Atomically revokes one active CLI session, its backing token, and records its audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for a foreign or unknown session, conflict when already revoked,
    /// validation for malformed input, or a provider failure.
    fn revoke_cli_session(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Atomically creates one scoped API token and its audit event without retaining the raw bearer
    /// value.
    ///
    /// # Errors
    ///
    /// Returns conflict for duplicate identifiers or hashes, validation for malformed input, or a
    /// provider failure.
    fn create_api_token(
        &self,
        token: &LocalApiTokenRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Lists all API-token metadata without raw bearer values.
    ///
    /// # Errors
    ///
    /// Returns a provider failure.
    fn list_api_tokens(&self) -> Result<Vec<LocalApiTokenRecord>, RepositoryError>;

    /// Resolves one active, unexpired API token by its digest and records successful use.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown or revoked token, validation for a malformed digest, or a
    /// provider failure.
    fn authenticate_api_token(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalApiTokenRecord, RepositoryError>;

    /// Atomically revokes an active API token exactly once and records its audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown token, conflict when already revoked, validation for a
    /// malformed identifier, or a provider failure.
    fn revoke_api_token(
        &self,
        id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;
}
