use crate::{NewAuditEvent, RepositoryError};
use unicode_normalization::UnicodeNormalization as _;

/// Maximum active groups in one workspace.
pub const MAXIMUM_ACTIVE_GROUPS: u32 = 500;
/// Maximum active members in one group.
pub const MAXIMUM_GROUP_MEMBERS: u32 = 500;
/// Maximum active group memberships for one user.
pub const MAXIMUM_USER_GROUPS: u32 = 100;
/// Maximum persisted active Yard grants targeting one group.
pub const MAXIMUM_ACTIVE_GROUP_GRANTS: u32 = 500;

/// Persisted lifecycle state of one workspace group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceGroupStatus {
    /// The group may hold members and admit users.
    Active,
    /// The group is tombstoned and admits nobody.
    Deactivated,
}

impl WorkspaceGroupStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        if matches!(self, Self::Active) {
            "active"
        } else {
            "deactivated"
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if value == Self::Active.as_str() {
            Some(Self::Active)
        } else if value == Self::Deactivated.as_str() {
            Some(Self::Deactivated)
        } else {
            None
        }
    }
}

/// Durable workspace-scoped group metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupRecord {
    /// Stable group identifier.
    pub id: String,
    /// Owning workspace identifier.
    pub workspace_id: String,
    /// Normalized human-readable label.
    pub name: String,
    /// Persisted lifecycle state.
    pub status: WorkspaceGroupStatus,
    /// Current active membership count.
    pub member_count: u32,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Deactivation time for a tombstoned group.
    pub deactivated_at_ms: Option<u64>,
}

/// Keyset position for newest-first group listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupCursor {
    /// Creation time of the final prior item.
    pub created_at_ms: u64,
    /// Identifier of the final prior item.
    pub id: String,
}

/// One page of workspace groups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupPage {
    /// Page items in newest-first order.
    pub items: Vec<WorkspaceGroupRecord>,
    /// Position after which the next page starts.
    pub next_cursor: Option<WorkspaceGroupCursor>,
}

/// One current group membership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupMemberRecord {
    /// Parent group identifier.
    pub group_id: String,
    /// Shared workspace boundary.
    pub workspace_id: String,
    /// Active local-user identifier.
    pub user_id: String,
    /// Membership creation time as Unix milliseconds.
    pub added_at_ms: u64,
}

/// Keyset position for newest-first group-member listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupMemberCursor {
    /// Addition time of the final prior item.
    pub added_at_ms: u64,
    /// User identifier of the final prior item.
    pub user_id: String,
}

/// One page of current group memberships.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGroupMemberPage {
    /// Page items in newest-first order.
    pub items: Vec<WorkspaceGroupMemberRecord>,
    /// Position after which the next page starts.
    pub next_cursor: Option<WorkspaceGroupMemberCursor>,
}

/// Normalizes and validates a workspace group name.
///
/// # Errors
///
/// Returns invalid input unless the NFC-normalized, Unicode-whitespace-trimmed value contains
/// between 2 and 80 scalar values and no control scalar.
pub fn normalize_group_name(value: &str) -> Result<String, RepositoryError> {
    let normalized = value.nfc().collect::<String>();
    let trimmed = normalized.trim_matches(char::is_whitespace);
    let valid =
        (2..=80).contains(&trimmed.chars().count()) && !trimmed.chars().any(char::is_control);
    if valid {
        Ok(trimmed.to_owned())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

/// Durable workspace group and membership operations.
pub trait WorkspaceGroupRepository: Send + Sync {
    /// Atomically creates one empty active group and its exact audit event.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when validation, capacity, or persistence fails.
    fn create_workspace_group(
        &self,
        group: &WorkspaceGroupRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Lists active and deactivated groups using stable newest-first keyset pagination.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when the scope, cursor, limit, or persistence fails.
    fn list_workspace_groups(
        &self,
        workspace_id: &str,
        cursor: Option<&WorkspaceGroupCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupPage, RepositoryError>;

    /// Atomically renames one active same-workspace group and records its exact audit event.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when validation, lookup, conflict, or persistence fails.
    fn rename_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        name: &str,
        event: &NewAuditEvent,
    ) -> Result<WorkspaceGroupRecord, RepositoryError>;

    /// Lists current members of one active same-workspace group using stable keyset pagination.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when scope, lookup, cursor, limit, or persistence fails.
    fn list_workspace_group_members(
        &self,
        workspace_id: &str,
        group_id: &str,
        cursor: Option<&WorkspaceGroupMemberCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupMemberPage, RepositoryError>;

    /// Atomically adds one active same-workspace user and records its exact audit event.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when validation, lookup, capacity, or persistence fails.
    fn add_workspace_group_member(
        &self,
        member: &WorkspaceGroupMemberRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Atomically removes one membership and records its exact audit event.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when validation, lookup, conflict, or persistence fails.
    fn remove_workspace_group_member(
        &self,
        workspace_id: &str,
        group_id: &str,
        user_id: &str,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Atomically tombstones one group, deletes memberships, revokes grants, and audits once.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when validation, lookup, conflict, or persistence fails.
    fn deactivate_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;
}

#[cfg(test)]
#[path = "workspace_groups_tests.rs"]
mod tests;
