use crate::RepositoryError;

/// Redaction-safe audit metadata value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditValue {
    /// Text that is safe to persist and return.
    String(String),
    /// Non-negative numeric metadata.
    Number(u64),
    /// Boolean metadata.
    Boolean(bool),
    /// Explicit null metadata.
    Null,
}

/// Append-only local audit event input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAuditEvent {
    /// Stable public event identifier.
    pub id: String,
    /// Authorized workspace identifier.
    pub workspace_id: String,
    /// Safe local principal label.
    pub actor: String,
    /// Stable action name.
    pub action: String,
    /// Request correlation identifier.
    pub request_id: String,
    /// Redaction-safe target class.
    pub target_type: String,
    /// Allowlisted operation metadata.
    pub metadata: Vec<(String, AuditValue)>,
    /// Event time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Persisted local audit event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEventRecord {
    /// Stable ordering sequence.
    pub sequence: u64,
    /// Stable action name.
    pub action: String,
    /// Safe local principal label.
    pub actor: String,
    /// Event time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Stable public event identifier.
    pub id: String,
    /// Allowlisted operation metadata.
    pub metadata: Vec<(String, AuditValue)>,
    /// Request correlation identifier.
    pub request_id: String,
    /// Redaction-safe target class.
    pub target_type: String,
    /// Authorized workspace identifier.
    pub workspace_id: String,
}

/// Stable page of newest-first audit events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditPage {
    /// Events in newest-first order.
    pub items: Vec<AuditEventRecord>,
    /// Sequence before which the next page starts.
    pub next_before: Option<u64>,
}

/// Exact object deletion target within an authorized project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectDeletionTarget {
    /// Parent project identifier.
    pub project_id: String,
    /// Logical object path.
    pub object_path: String,
    /// Optional immutable version number.
    pub version: Option<u64>,
}

/// One durable object deletion request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewObjectDeletion {
    /// Stable deletion operation identifier.
    pub id: String,
    /// Authorized target.
    pub target: ObjectDeletionTarget,
    /// Safe local principal label.
    pub actor: String,
    /// Request correlation identifier.
    pub request_id: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Storage work durably bound to a deletion operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletionItem {
    /// Object-version identifier.
    pub version_id: String,
    /// Provider-independent storage key.
    pub storage_key: String,
    /// Immutable version number.
    pub version: u64,
}

/// Durable deletion plan returned before byte mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletionPlan {
    /// Stable operation identifier.
    pub id: String,
    /// Storage keys that must be absent before finalization.
    pub items: Vec<DeletionItem>,
    /// Whether metadata finalization already completed.
    pub complete: bool,
    /// Safe actor bound when the plan was first persisted.
    pub actor: String,
    /// Request correlation identifier bound to the plan.
    pub request_id: String,
}

/// Deterministic project retention policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPolicyRecord {
    /// Parent project identifier.
    pub project_id: String,
    /// Number of newest matching versions preserved.
    pub keep_latest: u32,
    /// Optional logical-path glob.
    pub path_glob: Option<String>,
    /// Optional source-branch glob.
    pub branch_glob: Option<String>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Last update time as Unix milliseconds.
    pub updated_at_ms: u64,
}

/// Durable retention enforcement run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionRunRecord {
    /// Stable run identifier.
    pub id: String,
    /// Number of eligible matching versions considered.
    pub candidate_count: u64,
    /// Number of versions deleted.
    pub deleted_count: u64,
    /// Stable run status.
    pub status: String,
    /// Start time as Unix milliseconds.
    pub started_at_ms: u64,
    /// Completion time when terminal.
    pub completed_at_ms: Option<u64>,
    /// Redaction-safe failure summary.
    pub error_summary: Option<String>,
}

/// Policy and latest-run state for one project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionOverview {
    /// Enabled policy, when present.
    pub policy: Option<RetentionPolicyRecord>,
    /// Latest run, when present.
    pub last_run: Option<RetentionRunRecord>,
}

/// Durable object lifecycle, retention, and audit operations.
pub trait LifecycleRepository: Send + Sync {
    /// Records one standalone audit event atomically.
    ///
    /// # Errors
    ///
    /// Returns validation, conflict, or persistence failures.
    fn record_audit(&self, event: &NewAuditEvent) -> Result<(), RepositoryError>;
    /// Lists authorized audit events in stable newest-first pages.
    ///
    /// # Errors
    ///
    /// Returns validation or persistence failures.
    fn list_audit(
        &self,
        workspace_id: &str,
        before: Option<u64>,
        limit: u32,
    ) -> Result<AuditPage, RepositoryError>;
    /// Persists an exact deletion plan before any byte mutation.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or persistence failures.
    fn begin_object_deletion(
        &self,
        deletion: &NewObjectDeletion,
    ) -> Result<DeletionPlan, RepositoryError>;
    /// Atomically removes planned metadata and records the completed action.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn finish_deletion(
        &self,
        operation_id: &str,
        completed_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;
    /// Reads one enabled retention policy.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn retention_policy(&self, project_id: &str) -> Result<RetentionPolicyRecord, RepositoryError>;
    /// Replaces a retention policy and records the action atomically.
    ///
    /// # Errors
    ///
    /// Returns conflict, validation, or persistence failures.
    fn set_retention(
        &self,
        policy: &RetentionPolicyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;
    /// Disables a retention policy and records the action atomically.
    ///
    /// # Errors
    ///
    /// Returns conflict, validation, or persistence failures.
    fn clear_retention(
        &self,
        project_id: &str,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;
    /// Reads the enabled policy and latest durable run.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn retention_overview(&self, project_id: &str) -> Result<RetentionOverview, RepositoryError>;
    /// Plans one deterministic retention run before deleting bytes.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn begin_retention(
        &self,
        project_id: &str,
        run_id: &str,
        actor: &str,
        request_id: &str,
        started_at_ms: u64,
    ) -> Result<DeletionPlan, RepositoryError>;
    /// Marks an interrupted retention run failed without deleting metadata.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn fail_retention(&self, run_id: &str, completed_at_ms: u64) -> Result<(), RepositoryError>;
    /// Lists project identifiers with enabled retention policies.
    ///
    /// # Errors
    ///
    /// Returns persistence failures.
    fn retained_projects(&self) -> Result<Vec<String>, RepositoryError>;
}
