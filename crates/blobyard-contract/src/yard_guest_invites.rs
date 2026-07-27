use crate::{NewAuditEvent, NewYardAccessGrant, NewYardContinuation, RepositoryError};

/// Maximum retained invitations returned by one management page.
pub const YARD_GUEST_INVITE_PAGE_SIZE: usize = 50;
/// Maximum pending or accepted invitations for one Yard.
pub const MAXIMUM_ACTIVE_YARD_GUEST_INVITES: usize = 100;
/// Default invitation lifetime.
pub const YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS: u64 = 604_800_000;
/// Minimum invitation lifetime.
pub const YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS: u64 = 300_000;
/// Maximum invitation lifetime.
pub const YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS: u64 = 2_592_000_000;

/// Stable runtime-subject kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardSubjectKind {
    /// Existing active local user.
    Member,
    /// Accepted Yard guest invitation.
    Guest,
}

impl YardSubjectKind {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Guest => "guest",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "member" => Some(Self::Member),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }
}

/// Durable runtime subject used by continuations and Yard sessions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardSubjectRecord {
    /// Stable opaque subject identifier.
    pub id: String,
    /// Subject authority kind.
    pub kind: YardSubjectKind,
    /// Owning workspace boundary.
    pub workspace_id: String,
    /// Linked local user for member subjects.
    pub local_user_id: Option<String>,
    /// Linked accepted invitation for guest subjects.
    pub invitation_id: Option<String>,
    /// Subject creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Subject revocation time.
    pub revoked_at_ms: Option<u64>,
}

/// Persisted guest-invitation lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardGuestInviteStatus {
    /// Invitation token may be accepted once.
    Pending,
    /// Invitation is bound to one guest subject.
    Accepted,
    /// Invitation and its authority are tombstoned.
    Revoked,
}

impl YardGuestInviteStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Revoked => "revoked",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Decoded deterministic invitation-list cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardGuestInviteCursor {
    /// Creation time of the last returned invitation.
    pub created_at_ms: u64,
    /// Identifier of the last returned invitation.
    pub id: String,
}

/// Validated input for one pending guest invitation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardGuestInvite {
    /// Stable public invitation identifier.
    pub id: String,
    /// Workspace tenant boundary.
    pub workspace_id: String,
    /// Project tenant boundary.
    pub project_id: String,
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Normalized invited email.
    pub email: String,
    /// Lowercase SHA-256 digest of the raw invitation token.
    pub token_hash: String,
    /// Stable access grant created in the same transaction.
    pub grant_id: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute invitation expiry.
    pub expires_at_ms: u64,
}

/// Non-secret durable guest invitation projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardGuestInviteRecord {
    /// Stable public invitation identifier.
    pub id: String,
    /// Workspace tenant boundary.
    pub workspace_id: String,
    /// Project tenant boundary.
    pub project_id: String,
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Normalized invited email.
    pub email: String,
    /// Persisted lifecycle state.
    pub status: YardGuestInviteStatus,
    /// Accepted Core guest subject, never exposed by management presentation.
    pub accepted_subject_id: Option<String>,
    /// Stable matching access-grant identifier.
    pub grant_id: String,
    /// Application roles stored on the matching grant.
    pub app_roles: Vec<String>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute invitation expiry.
    pub expires_at_ms: u64,
    /// Acceptance time.
    pub accepted_at_ms: Option<u64>,
    /// Revocation time.
    pub revoked_at_ms: Option<u64>,
}

/// One bounded deterministic invitation page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardGuestInvitePage {
    /// Ordered non-secret invitations.
    pub items: Vec<YardGuestInviteRecord>,
    /// Cursor for the next page.
    pub next_cursor: Option<YardGuestInviteCursor>,
}

/// One hashed guest login key stored without its raw value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardGuestLoginKeyRecord {
    /// Stable key identifier.
    pub id: String,
    /// Accepted guest subject.
    pub subject_id: String,
    /// Accepted invitation.
    pub invitation_id: String,
    /// Workspace tenant boundary.
    pub workspace_id: String,
    /// Non-secret prefix for internal diagnostics.
    pub token_prefix: String,
    /// Lowercase SHA-256 digest of the raw guest key.
    pub secret_hash: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute key expiry.
    pub expires_at_ms: u64,
    /// Most recent successful Yard-login use.
    pub last_used_at_ms: Option<u64>,
    /// Revocation time.
    pub revoked_at_ms: Option<u64>,
}

/// Atomic accepted-invitation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardGuestAcceptance {
    /// Accepted invitation.
    pub invitation: YardGuestInviteRecord,
    /// New guest subject.
    pub subject: YardSubjectRecord,
}

/// Durable guest invitations, subjects, and Yard-login keys.
pub trait YardGuestRepository: Send + Sync {
    /// Lists one deterministic invitation page.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_yard_guest_invites(
        &self,
        yard_id: &str,
        cursor: Option<&YardGuestInviteCursor>,
        limit: usize,
    ) -> Result<YardGuestInvitePage, RepositoryError>;

    /// Reads one non-secret invitation by stable identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown invitation, validation for malformed input, or a provider
    /// failure.
    fn yard_guest_invite_by_id(
        &self,
        invitation_id: &str,
    ) -> Result<YardGuestInviteRecord, RepositoryError>;

    /// Atomically creates one invitation, matching grant, and exact audit event.
    ///
    /// # Errors
    ///
    /// Returns conflict for duplicate live authority, validation for malformed input, not-found
    /// for foreign durable bindings, or a provider failure.
    fn create_yard_guest_invite(
        &self,
        invitation: &NewYardGuestInvite,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError>;

    /// Resolves one live pending invitation by token hash for account-origin presentation.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, accepted, revoked, or expired authority, validation for
    /// malformed input, or a provider failure.
    fn pending_yard_guest_invite_by_token(
        &self,
        token_hash: &str,
        now_ms: u64,
    ) -> Result<YardGuestInviteRecord, RepositoryError>;

    /// Atomically accepts one invitation, creates its subject and key, stores a continuation,
    /// consumes the invitation token, and records the exact audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown or unusable authority, conflict for duplicate durable
    /// material, validation for malformed input, or a provider failure.
    fn accept_yard_guest_invite(
        &self,
        token_hash: &str,
        subject: &YardSubjectRecord,
        key: &YardGuestLoginKeyRecord,
        continuation: &NewYardContinuation,
        event: &NewAuditEvent,
        now_ms: u64,
    ) -> Result<YardGuestAcceptance, RepositoryError>;

    /// Atomically revokes one invitation, its grant, active guest keys, and exact audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found for an unknown or foreign invitation, validation for malformed input, or
    /// a provider failure.
    fn revoke_yard_guest_invite(
        &self,
        yard_id: &str,
        invitation_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError>;

    /// Authenticates one active guest key for the account-origin Yard login flow only.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, or revoked authority, validation for malformed
    /// input, or a provider failure.
    fn authenticate_yard_guest_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<YardSubjectRecord, RepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::{YardGuestInviteStatus, YardSubjectKind};

    #[test]
    fn guest_enums_round_trip_and_reject_unknown_values() {
        for (kind, encoded) in [
            (YardSubjectKind::Member, "member"),
            (YardSubjectKind::Guest, "guest"),
        ] {
            assert_eq!(kind.as_str(), encoded);
            assert_eq!(YardSubjectKind::parse(encoded), Some(kind));
        }
        assert_eq!(YardSubjectKind::parse("unknown"), None);

        for (status, encoded) in [
            (YardGuestInviteStatus::Pending, "pending"),
            (YardGuestInviteStatus::Accepted, "accepted"),
            (YardGuestInviteStatus::Revoked, "revoked"),
        ] {
            assert_eq!(status.as_str(), encoded);
            assert_eq!(YardGuestInviteStatus::parse(encoded), Some(status));
        }
        assert_eq!(YardGuestInviteStatus::parse("unknown"), None);
    }
}
