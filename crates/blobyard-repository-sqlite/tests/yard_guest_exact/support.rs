use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardAccessGrant, NewYardGuestInvite,
    YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS, YardAccessPrincipalKind, YardGuestLoginKeyRecord,
    YardGuestRepository, YardSubjectKind, YardSubjectRecord,
};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_testkit::{SQLITE_GUEST_YARD_SEED, sqlite_guest_yard_continuation};

const INVITATION_ID: &str = "ygi_00000000000000000000000000000001";
const SUBJECT_ID: &str = "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Fixture {
    _temporary: tempfile::TempDir,
    path: std::path::PathBuf,
    repository: SqliteRepository,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join("metadata.sqlite3");
        let repository = SqliteRepository::open(&path).expect("repository");
        let connection = rusqlite::Connection::open(&path).expect("seed connection");
        connection
            .execute_batch(SQLITE_GUEST_YARD_SEED)
            .expect("fixture");
        Self {
            _temporary: temporary,
            path,
            repository,
        }
    }

    fn create(&self, index: u64) {
        let invitation = invitation(index);
        self.repository
            .create_yard_guest_invite(
                &invitation,
                &grant(&invitation),
                &event("created", &invitation, None, invitation.created_at_ms),
            )
            .expect("create invitation");
    }

    fn create_and_accept(&self) {
        let invitation = invitation(1);
        self.create(1);
        self.repository
            .accept_yard_guest_invite(
                &hash('c'),
                &subject(),
                &key(invitation.expires_at_ms),
                &sqlite_guest_yard_continuation(),
                &event("accepted", &invitation, Some(SUBJECT_ID), 2),
                2,
            )
            .expect("accept invitation");
    }
}

fn invitation_id(index: u64) -> String {
    format!("ygi_{index:032x}")
}

fn invitation(index: u64) -> NewYardGuestInvite {
    let created_at_ms = if index == 1 { 1 } else { 1_000 + index };
    NewYardGuestInvite {
        id: invitation_id(index),
        workspace_id: "workspace_guest".to_owned(),
        project_id: "project_guest".to_owned(),
        yard_id: "yard_guest".to_owned(),
        environment_id: Some("environment_guest".to_owned()),
        email: format!("guest{index}@example.test"),
        token_hash: if index == 1 {
            hash('c')
        } else {
            format!("{index:064x}")
        },
        grant_id: format!("yardgrant_{index:032x}"),
        created_at_ms,
        expires_at_ms: created_at_ms + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS,
    }
}

fn grant(invitation: &NewYardGuestInvite) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: invitation.grant_id.clone(),
        yard_id: invitation.yard_id.clone(),
        environment_id: invitation.environment_id.clone(),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: invitation.id.clone(),
        app_roles: Vec::new(),
        created_at_ms: invitation.created_at_ms,
        created_by_principal: "operator".to_owned(),
        expires_at_ms: Some(invitation.expires_at_ms),
    }
}

fn subject() -> YardSubjectRecord {
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

fn key(expires_at_ms: u64) -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: "yardguestkey_guest".to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        invitation_id: INVITATION_ID.to_owned(),
        workspace_id: "workspace_guest".to_owned(),
        token_prefix: "byg_fixture".to_owned(),
        secret_hash: hash('d'),
        created_at_ms: 2,
        expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn event(
    action: &str,
    invitation: &NewYardGuestInvite,
    subject_id: Option<&str>,
    at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_guest_{action}_{}", invitation.id),
        workspace_id: invitation.workspace_id.clone(),
        actor: "operator".to_owned(),
        action: format!("yard.guest_invite.{action}"),
        request_id: format!("request_guest_{action}_{}", invitation.id),
        target_type: "yard_guest_invite".to_owned(),
        metadata: vec![
            (
                "environmentId".to_owned(),
                invitation
                    .environment_id
                    .clone()
                    .map_or(AuditValue::Null, AuditValue::String),
            ),
            (
                "grantId".to_owned(),
                AuditValue::String(invitation.grant_id.clone()),
            ),
            (
                "invitationId".to_owned(),
                AuditValue::String(invitation.id.clone()),
            ),
            (
                "projectId".to_owned(),
                AuditValue::String(invitation.project_id.clone()),
            ),
            (
                "subjectId".to_owned(),
                subject_id.map_or(AuditValue::Null, |id| AuditValue::String(id.to_owned())),
            ),
            (
                "yardId".to_owned(),
                AuditValue::String(invitation.yard_id.clone()),
            ),
        ],
        created_at_ms: at_ms,
    }
}

fn hash(character: char) -> String {
    character.to_string().repeat(64)
}

#[test]
fn exact_guest_query_failures_execute_in_the_public_library_copy() {
    assert_invalid_guest_queries();
    assert_corrupt_guest_queries_fail_closed();
    assert_missing_guest_table_fails_closed();
}
