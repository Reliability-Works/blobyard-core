use crate::{NewAuditEvent, RepositoryError, YardAccessGrantRecord};
use blobyard_core::{ApplicationPolicyGraph, EffectiveApplicationPolicy};

/// Maximum active management-role assignments for one Yard.
pub const MAXIMUM_YARD_MANAGEMENT_ROLES: u16 = 500;
/// Maximum application roles stored on one Yard access grant.
pub const MAXIMUM_YARD_ACCESS_ROLES: usize = 16;

/// One Yard-scoped human management role.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum YardManagementRole {
    /// Full Yard authority, including policy approval and role assignment.
    Owner,
    /// Yard access and operational authority.
    Admin,
    /// Yard deployment and operational authority.
    Developer,
    /// Read-only Yard and audit authority.
    Auditor,
}

impl YardManagementRole {
    /// Returns the stable persisted and API representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Developer => "developer",
            Self::Auditor => "auditor",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Self::Owner),
            "admin" => Some(Self::Admin),
            "developer" => Some(Self::Developer),
            "auditor" => Some(Self::Auditor),
            _ => None,
        }
    }

    /// Returns the deterministic listing precedence.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Owner => 0,
            Self::Admin => 1,
            Self::Developer => 2,
            Self::Auditor => 3,
        }
    }
}

/// Durable Yard management-role assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardManagementRoleAssignment {
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Assigned active local-user identifier.
    pub user_id: String,
    /// Same-workspace tenant boundary.
    pub workspace_id: String,
    /// Assigned management role.
    pub role: YardManagementRole,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Last change time as Unix milliseconds.
    pub updated_at_ms: u64,
}

/// Decoded keyset cursor for deterministic management-role listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardManagementRoleCursor {
    /// Role precedence of the last returned assignment.
    pub role: YardManagementRole,
    /// User identifier of the last returned assignment.
    pub user_id: String,
}

/// One bounded management-role page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardManagementRolePage {
    /// Ordered page items.
    pub items: Vec<YardManagementRoleAssignment>,
    /// Cursor for the next page when more assignments exist.
    pub next_cursor: Option<YardManagementRoleCursor>,
}

/// Durable owner-approved application policy and deterministic closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardApplicationPolicyRecord {
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Same-workspace tenant boundary.
    pub workspace_id: String,
    /// Monotonic policy revision.
    pub revision: u64,
    /// Digest of the canonical source manifest.
    pub source_manifest_digest: String,
    /// Canonical declared role graph.
    pub policy: ApplicationPolicyGraph,
    /// Persisted deterministic closure.
    pub effective: EffectiveApplicationPolicy,
    /// Approval time as Unix milliseconds.
    pub approved_at_ms: u64,
    /// Safe Core operator principal identifier that approved the policy.
    pub approved_by_principal: String,
}

/// Sanitised live identity exposed only on an admitted private Yard origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardIdentity {
    /// Current local-user identifier.
    pub user_id: String,
    /// Current workspace tenant boundary.
    pub workspace_id: String,
    /// Current project tenant boundary.
    pub project_id: String,
    /// Current Yard identifier.
    pub yard_id: String,
    /// Current environment identifier.
    pub environment_id: String,
    /// Current optional display name.
    pub display_name: Option<String>,
    /// Current optional email.
    pub email: Option<String>,
    /// Only current groups whose matching grants contribute to this Yard.
    pub groups: Vec<String>,
    /// Current independent Yard management role.
    pub management_role: Option<YardManagementRole>,
    /// Sorted effective application-role closure.
    pub app_roles: Vec<String>,
    /// Sorted effective application permissions.
    pub permissions: Vec<String>,
    /// Current host-bound Yard session identifier.
    pub session_id: String,
}

/// Durable Yard management roles, approved application policy, and live identity operations.
pub trait YardIdentityRepository: Send + Sync {
    /// Lists one deterministic page of management-role assignments.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown Yard, validation for a malformed cursor, or a provider
    /// failure.
    fn list_yard_management_roles(
        &self,
        yard_id: &str,
        cursor: Option<&YardManagementRoleCursor>,
    ) -> Result<YardManagementRolePage, RepositoryError>;

    /// Atomically creates or changes one assignment and records exactly one audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for foreign resources, conflict for limits or the last-owner invariant,
    /// validation for malformed input, or a provider failure.
    fn set_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        role: YardManagementRole,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardManagementRoleAssignment, RepositoryError>;

    /// Atomically revokes one assignment and records exactly one audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for foreign resources, conflict for the last-owner invariant, validation
    /// for malformed input, or a provider failure.
    fn revoke_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Reads one current approved policy, or `None` when no policy exists.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown Yard, validation for corrupt policy state, or a provider
    /// failure.
    fn get_yard_application_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardApplicationPolicyRecord>, RepositoryError>;

    /// Canonicalizes and atomically approves one policy revision with exactly one audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for a foreign Yard, conflict for missing ownership, validation for an
    /// invalid graph or digest, or a provider failure.
    fn set_yard_application_policy(
        &self,
        yard_id: &str,
        source_manifest_digest: &str,
        policy: ApplicationPolicyGraph,
        approved_by_principal: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardApplicationPolicyRecord, RepositoryError>;

    /// Atomically replaces one active grant's application-role list with one audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for a foreign grant, conflict for an exceeded limit, validation for
    /// undeclared roles, or a provider failure.
    fn set_yard_access_roles(
        &self,
        yard_id: &str,
        grant_id: &str,
        app_roles: &[String],
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError>;

    /// Resolves a live, sanitised identity for one admitted private host-bound Yard session.
    ///
    /// # Errors
    ///
    /// Returns not-found for public, unknown, expired, revoked, or no-longer-admitted sessions,
    /// validation for corrupt identity state, or a provider failure.
    fn resolve_yard_identity(
        &self,
        host_label: &str,
        session_token_hash: &str,
        now_ms: u64,
    ) -> Result<YardIdentity, RepositoryError>;
}

#[cfg(test)]
#[path = "yard_identity_tests.rs"]
mod tests;
