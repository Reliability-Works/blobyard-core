#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::SqliteRepository;
use blobyard_contract::{
    NewYardAccessGrant, NewYardGuestInvite, NewYardSession, RepositoryError,
    YARD_SESSION_LIFETIME_MS, YardAccessPrincipalKind, YardGuestInviteStatus,
    YardGuestLoginKeyRecord, YardGuestRepository, YardIdentityRepository, YardSessionAuditContext,
    YardSessionRepository, YardSubjectKind, YardSubjectRecord,
};
use blobyard_testkit::{SQLITE_GUEST_YARD_SEED, sqlite_guest_yard_continuation};

#[path = "yard_guest_invites_test_events.rs"]
pub(in crate::adapter) mod events;
use events::{event, hash};

pub(in crate::adapter) const INVITATION_ID: &str = "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(in crate::adapter) const SUBJECT_ID: &str = "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub(in crate::adapter) const TOKEN_HASH: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
pub(in crate::adapter) const KEY_HASH: &str =
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
pub(super) const EXPIRES_AT_MS: u64 = 600_001;

pub(super) struct Fixture {
    _temporary: tempfile::TempDir,
    pub(super) repository: SqliteRepository,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let repository =
            SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
        repository
            .test_connection()
            .expect("connection")
            .execute_batch(SQLITE_GUEST_YARD_SEED)
            .expect("fixture");
        Self {
            _temporary: temporary,
            repository,
        }
    }

    pub(super) fn create(&self) {
        let invitation = invitation();
        self.repository
            .create_yard_guest_invite(
                &invitation,
                &grant(),
                &event("created", &invitation, None, 1),
            )
            .expect("create");
    }

    pub(super) fn accept(&self) {
        let invitation = invitation();
        self.repository
            .accept_yard_guest_invite(
                TOKEN_HASH,
                &subject(),
                &key(),
                &sqlite_guest_yard_continuation(),
                &event("accepted", &invitation, Some(SUBJECT_ID), 2),
                2,
            )
            .expect("accept");
    }
}

#[test]
fn guest_invitation_lifecycle_is_atomic_single_use_and_immediately_revocable() {
    let fixture = Fixture::new();
    assert_create_accept_and_admission(&fixture);
    assert_exchange_identity(&fixture);
    assert_revocation(&fixture);
}

fn assert_create_accept_and_admission(fixture: &Fixture) {
    fixture.create();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &invitation(),
            &grant(),
            &event("created", &invitation(), None, 1)
        ),
        Err(RepositoryError::Conflict)
    );
    fixture.accept();
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(TOKEN_HASH, 3),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .repository
            .authenticate_yard_guest_key(KEY_HASH, 3)
            .expect("authenticate")
            .id,
        SUBJECT_ID
    );
    assert!(
        fixture
            .repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3)
            .is_ok()
    );
}

fn assert_exchange_identity(fixture: &Fixture) {
    let exchange = fixture
        .repository
        .exchange_yard_session_code(
            &hash('f'),
            "guest-yard-fixture",
            &session(),
            &YardSessionAuditContext {
                id: "audit_guest_session".to_owned(),
                request_id: "request_guest_session".to_owned(),
            },
            3,
        )
        .expect("exchange");
    let identity = fixture
        .repository
        .resolve_yard_identity("guest-yard-fixture", &exchange.session.token_hash, 4)
        .expect("identity");
    assert_eq!(identity.user_id, SUBJECT_ID);
    assert_eq!(identity.display_name.as_deref(), Some("guest@example.test"));
    assert_eq!(identity.email.as_deref(), Some("guest@example.test"));
    assert!(identity.groups.is_empty());
    assert_eq!(identity.management_role, None);
    assert!(identity.app_roles.is_empty());

    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE yard_access_grants SET status = 'revoked', revoked_at_ms = 4
             WHERE id = 'yardgrant_guest'",
            [],
        )
        .expect("temporarily revoke grant");
    assert_eq!(
        fixture.repository.resolve_yard_identity(
            "guest-yard-fixture",
            &exchange.session.token_hash,
            4,
        ),
        Err(RepositoryError::NotFound)
    );
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE yard_access_grants SET status = 'active', revoked_at_ms = NULL
             WHERE id = 'yardgrant_guest'",
            [],
        )
        .expect("restore grant");
}

fn assert_revocation(fixture: &Fixture) {
    let revoked = fixture
        .repository
        .revoke_yard_guest_invite(
            "yard_guest",
            INVITATION_ID,
            5,
            &event("revoked", &invitation(), Some(SUBJECT_ID), 5),
        )
        .expect("revoke");
    assert_eq!(revoked.status, YardGuestInviteStatus::Revoked);
    assert_eq!(
        fixture.repository.authenticate_yard_guest_key(KEY_HASH, 6),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 6),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "yard_guest",
            INVITATION_ID,
            7,
            &event("revoked", &invitation(), Some(SUBJECT_ID), 7)
        ),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn guest_authority_denies_expiry_wrong_scope_modes_and_corrupt_subjects() {
    let fixture = Fixture::new();
    let mut wrong = invitation();
    wrong.workspace_id = "foreign_workspace".to_owned();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &wrong,
            &grant(),
            &event("created", &wrong, None, 1),
        ),
        Err(RepositoryError::InvalidInput)
    );
    fixture.create();
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(TOKEN_HASH, EXPIRES_AT_MS),
        Err(RepositoryError::NotFound)
    );
    fixture.accept();
    for visibility in ["workspace", "any-authenticated", "owner"] {
        fixture
            .repository
            .test_connection()
            .expect("connection")
            .execute(
                "UPDATE yard_access_policies SET visibility = ?1 WHERE yard_id = 'yard_guest'",
                [visibility],
            )
            .expect("visibility");
        assert_eq!(
            fixture
                .repository
                .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3),
            Err(RepositoryError::NotFound)
        );
    }
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE yard_subjects SET kind = 'unknown' WHERE id = 'guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';",
        )
        .expect("corrupt subject");
    assert_eq!(
        fixture
            .repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3),
        Err(RepositoryError::NotFound)
    );
}

pub(in crate::adapter) fn invitation() -> NewYardGuestInvite {
    NewYardGuestInvite {
        id: INVITATION_ID.to_owned(),
        workspace_id: "workspace_guest".to_owned(),
        project_id: "project_guest".to_owned(),
        yard_id: "yard_guest".to_owned(),
        environment_id: Some("environment_guest".to_owned()),
        email: "guest@example.test".to_owned(),
        token_hash: TOKEN_HASH.to_owned(),
        grant_id: "yardgrant_guest".to_owned(),
        created_at_ms: 1,
        expires_at_ms: EXPIRES_AT_MS,
    }
}

pub(in crate::adapter) fn grant() -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: "yardgrant_guest".to_owned(),
        yard_id: "yard_guest".to_owned(),
        environment_id: Some("environment_guest".to_owned()),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: INVITATION_ID.to_owned(),
        app_roles: Vec::new(),
        created_at_ms: 1,
        created_by_principal: "operator".to_owned(),
        expires_at_ms: Some(EXPIRES_AT_MS),
    }
}

pub(in crate::adapter) fn subject() -> YardSubjectRecord {
    YardSubjectRecord {
        id: SUBJECT_ID.to_owned(),
        kind: YardSubjectKind::Guest,
        workspace_id: "workspace_guest".to_owned(),
        local_user_id: None,
        invitation_id: Some(INVITATION_ID.to_owned()),
        created_at_ms: 2,
        revoked_at_ms: None,
    }
}

pub(in crate::adapter) fn key() -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: "yardguestkey_guest".to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        invitation_id: INVITATION_ID.to_owned(),
        workspace_id: "workspace_guest".to_owned(),
        token_prefix: "byg_dddddddd".to_owned(),
        secret_hash: KEY_HASH.to_owned(),
        created_at_ms: 2,
        expires_at_ms: EXPIRES_AT_MS,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

pub(super) fn session() -> NewYardSession {
    NewYardSession {
        id: "session_guest".to_owned(),
        token_hash: hash('1'),
        created_at_ms: 3,
        expires_at_ms: 3 + YARD_SESSION_LIFETIME_MS,
    }
}
