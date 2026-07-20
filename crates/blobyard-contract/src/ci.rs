use crate::{AuditValue, NewAuditEvent, RepositoryError};

/// One operation that a GitHub Actions trust may grant to a short-lived machine session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CiAction {
    /// Download stored objects.
    Download,
    /// Create and manage controlled shares.
    Share,
    /// Upload object bytes and complete their immutable versions.
    Upload,
    /// Create and manage public Web Yard deployments.
    YardManage,
}

impl CiAction {
    /// Returns the stable public action name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Share => "share",
            Self::Upload => "upload",
            Self::YardManage => "yard:manage",
        }
    }

    /// Parses one exact public action name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "download" => Some(Self::Download),
            "share" => Some(Self::Share),
            "upload" => Some(Self::Upload),
            "yard:manage" => Some(Self::YardManage),
            _ => None,
        }
    }
}

/// Returns whether one GitHub repository owner or name satisfies the shared trust syntax.
#[must_use]
pub fn valid_github_repository_part(value: &str, maximum: usize, extended: bool) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'-'
                || (extended && matches!(byte, b'.' | b'_'))
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

/// Returns whether one GitHub workflow path names a root workflow YAML file.
#[must_use]
pub fn valid_github_workflow_path(value: &str) -> bool {
    value
        .strip_prefix(".github/workflows/")
        .and_then(|name| {
            name.rsplit_once('.')
                .filter(|(_, extension)| matches!(*extension, "yml" | "yaml"))
                .map(|_| name)
        })
        .is_some_and(|name| {
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

/// Returns whether one named Git ref tail satisfies the shared safe character contract.
#[must_use]
pub fn valid_github_ref_tail(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-'))
}

/// Inputs for the canonical redacted audit event emitted by CI mutations.
pub struct NewCiAuditEvent {
    /// Stable audit event identifier.
    pub id: String,
    /// Workspace owning the mutated CI resource.
    pub workspace_id: String,
    /// Redacted actor identifier.
    pub actor: String,
    /// Canonical audit action.
    pub action: String,
    /// Request correlation identifier.
    pub request_id: String,
    /// Canonical target kind.
    pub target_type: String,
    /// Stable target identifier.
    pub target_id: String,
    /// Normalized GitHub repository.
    pub repository: String,
    /// Event creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Builds the canonical redacted audit event for one CI trust or machine-session mutation.
#[must_use]
pub fn ci_audit_event(input: NewCiAuditEvent) -> NewAuditEvent {
    NewAuditEvent {
        id: input.id,
        workspace_id: input.workspace_id,
        actor: input.actor,
        action: input.action,
        request_id: input.request_id,
        target_type: input.target_type,
        metadata: vec![
            (
                "repository".to_owned(),
                AuditValue::String(input.repository),
            ),
            ("targetId".to_owned(), AuditValue::String(input.target_id)),
        ],
        created_at_ms: input.created_at_ms,
    }
}

/// Durable GitHub Actions trust owned by one standalone workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCiTrustRecord {
    /// Stable public trust identifier.
    pub id: String,
    /// Workspace granted by the trust.
    pub workspace_id: String,
    /// Optional fixed project boundary.
    pub project_id: Option<String>,
    /// Normalized GitHub owner and repository.
    pub repository: String,
    /// Exact trusted workflow file path.
    pub workflow_path: String,
    /// Exact trusted workflow Git ref or commit.
    pub workflow_ref: String,
    /// Bounded Git ref glob matched against the invoking workflow ref.
    pub allowed_ref_glob: String,
    /// Optional exact GitHub environment.
    pub environment: Option<String>,
    /// Maximum actions that a derived session may request.
    pub allowed_actions: Vec<CiAction>,
    /// Exact OIDC audience configured when the trust was created.
    pub audience: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Revocation time when the trust is no longer active.
    pub revoked_at_ms: Option<u64>,
}

/// Verified, normalized identity extracted from one GitHub-signed OIDC assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubOidcIdentity {
    /// Exact verified audience.
    pub audience: String,
    /// Normalized GitHub owner and repository.
    pub repository: String,
    /// Exact invoking Git ref.
    pub git_ref: String,
    /// Exact invoking workflow file path.
    pub workflow_path: String,
    /// Exact invoking workflow Git ref or commit.
    pub workflow_ref: String,
    /// Optional exact GitHub environment.
    pub environment: Option<String>,
    /// GitHub workflow run identifier.
    pub run_id: String,
    /// Optional GitHub workflow retry number.
    pub run_attempt: Option<String>,
    /// Optional invoking commit SHA.
    pub sha: Option<String>,
    /// Absolute assertion expiry as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// One requested machine-session exchange after signature and claim verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMachineSession {
    /// Stable machine-session and backing-token identifier.
    pub id: String,
    /// Non-secret bearer prefix retained for diagnostics.
    pub token_prefix: String,
    /// Lowercase SHA-256 digest of the raw machine bearer.
    pub secret_hash: String,
    /// Verified GitHub identity.
    pub identity: GithubOidcIdentity,
    /// Optional requested workspace slug.
    pub workspace: Option<String>,
    /// Required requested project slug.
    pub project: String,
    /// Exact requested actions.
    pub actions: Vec<CiAction>,
    /// Lowercase SHA-256 digest of the source OIDC assertion.
    pub oidc_token_hash: String,
    /// Current time as Unix milliseconds.
    pub now_ms: u64,
}

/// Persisted machine-session metadata without either raw bearer credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalMachineSessionRecord {
    /// Stable session and backing token identifier.
    pub id: String,
    /// Trust that minted this session.
    pub trust_id: String,
    /// Authorized workspace identifier.
    pub workspace_id: String,
    /// Authorized project identifier.
    pub project_id: String,
    /// Normalized source repository.
    pub repository: String,
    /// Invoking Git ref.
    pub git_ref: String,
    /// GitHub workflow run identifier.
    pub run_id: String,
    /// Optional workflow retry number.
    pub run_attempt: Option<String>,
    /// Granted actions.
    pub actions: Vec<CiAction>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Most recent successful authentication.
    pub last_used_at_ms: Option<u64>,
    /// Revocation time when the session is no longer active.
    pub revoked_at_ms: Option<u64>,
}

/// Atomic outcome of a GitHub OIDC machine-session exchange.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MachineSessionMintResult {
    /// The assertion digest was already exchanged.
    Replayed,
    /// No active trust authorized every verified claim and requested action.
    Forbidden,
    /// The matching trust exceeded its durable exchange window.
    RateLimited {
        /// Minimum whole seconds before another exchange may succeed.
        retry_after_seconds: u64,
    },
    /// A hash-only machine session was committed.
    Minted(Box<LocalMachineSessionRecord>),
}

/// Durable standalone GitHub OIDC trust and short-lived machine-session operations.
pub trait CiRepository: Send + Sync {
    /// Atomically creates one validated trust and its audit event.
    ///
    /// # Errors
    ///
    /// Returns conflict, validation, not-found, or provider failures.
    fn create_ci_trust(
        &self,
        trust: &LocalCiTrustRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Lists all trusts for one workspace in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_ci_trusts(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCiTrustRecord>, RepositoryError>;

    /// Atomically revokes one trust, every derived session and token, and records audit.
    ///
    /// Returns `true` when this call performed revocation and `false` when it was already revoked.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn revoke_ci_trust(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;

    /// Atomically matches an active trust and mints one replay-safe machine session.
    ///
    /// # Errors
    ///
    /// Returns validation, conflict, or provider failures.
    fn mint_machine_session(
        &self,
        session: &NewMachineSession,
        event: &NewAuditEvent,
    ) -> Result<MachineSessionMintResult, RepositoryError>;

    /// Resolves and records use of one active machine session by its stable token identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found for an invalid, expired, revoked, or trust-invalidated session, validation
    /// for malformed input, or a provider failure.
    fn authenticate_machine_session(
        &self,
        token_id: &str,
        now_ms: u64,
    ) -> Result<LocalMachineSessionRecord, RepositoryError>;
}
