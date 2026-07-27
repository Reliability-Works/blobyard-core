#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "fault sweeps keep each atomic guest-authority mutation and rollback assertion together"
)]

use super::SqliteRepository;
use blobyard_contract::{
    RepositoryError, YardGuestInviteStatus, YardGuestRepository, YardSessionRepository,
};
use std::sync::atomic::Ordering;

use super::super::yard_guest_invites::tests::events::event;
use super::super::yard_guest_invites::tests::{
    INVITATION_ID, KEY_HASH, SUBJECT_ID, TOKEN_HASH, grant, invitation, key, subject,
};

#[test]
fn injected_faults_roll_back_every_guest_authority_mutation_and_audit() {
    let create_boundaries = sweep(|_repository| {}, create_operation, verify_create_rollback);
    let accept_boundaries = sweep(seed_invitation, accept_operation, verify_accept_rollback);
    let revoke_boundaries = sweep(seed_acceptance, revoke_operation, verify_revoke_rollback);

    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "guest-atomicity");
    for (id, mutation, boundaries) in [
        (
            "guest-create-rolls-back-after-every-write-boundary",
            "create",
            create_boundaries,
        ),
        (
            "guest-accept-rolls-back-after-every-write-boundary",
            "accept",
            accept_boundaries,
        ),
        (
            "guest-revoke-rolls-back-after-every-write-boundary",
            "revoke",
            revoke_boundaries,
        ),
    ] {
        assert!(boundaries > 0, "{mutation} must perform durable writes");
        tracker.record_case(
            id,
            &serde_json::json!({
                "mutation": mutation,
                "faultPosition": "each-write-authorization-boundary"
            }),
            &serde_json::json!({
                "rolledBack": true,
                "auditEventCreated": false,
                "writeBoundaryCoverage": "complete"
            }),
        );
    }
    tracker.finish().expect("complete guest atomicity fixtures");
}

fn sweep(
    setup: impl Fn(&SqliteRepository),
    operation: impl Fn(&SqliteRepository) -> Result<(), RepositoryError>,
    verify: impl Fn(&SqliteRepository),
) -> usize {
    let probe = fault_repository();
    setup(&probe);
    let observed = {
        let connection = probe.test_connection().expect("connection");
        super::install_denial(&connection, usize::MAX)
    };
    operation(&probe).expect("operation succeeds without injected denial");
    let write_boundaries = observed.load(Ordering::Relaxed);

    let repository = fault_repository();
    setup(&repository);
    for denied_index in 0..write_boundaries {
        let observed = {
            let connection = repository.test_connection().expect("connection");
            super::install_denial(&connection, denied_index)
        };
        assert_eq!(operation(&repository), Err(RepositoryError::Unavailable));
        assert!(observed.load(Ordering::Relaxed) > denied_index);
        verify(&repository);
    }
    write_boundaries
}

fn fault_repository() -> SqliteRepository {
    let connection = rusqlite::Connection::open_in_memory().expect("connection");
    let repository = SqliteRepository::initialize_connection(connection).expect("repository");
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(blobyard_testkit::SQLITE_GUEST_YARD_SEED)
        .expect("guest fixture");
    repository
}

fn seed_invitation(repository: &SqliteRepository) {
    create_operation(repository).expect("invitation");
}

fn seed_acceptance(repository: &SqliteRepository) {
    seed_invitation(repository);
    accept_operation(repository).expect("acceptance");
}

fn create_operation(repository: &SqliteRepository) -> Result<(), RepositoryError> {
    repository
        .create_yard_guest_invite(
            &invitation(),
            &grant(),
            &event("created", &invitation(), None, 1),
        )
        .map(|_record| ())
}

fn accept_operation(repository: &SqliteRepository) -> Result<(), RepositoryError> {
    repository
        .accept_yard_guest_invite(
            TOKEN_HASH,
            &subject(),
            &key(),
            &blobyard_testkit::sqlite_guest_yard_continuation(),
            &event("accepted", &invitation(), Some(SUBJECT_ID), 2),
            2,
        )
        .map(|_acceptance| ())
}

fn revoke_operation(repository: &SqliteRepository) -> Result<(), RepositoryError> {
    repository
        .revoke_yard_guest_invite(
            "yard_guest",
            INVITATION_ID,
            3,
            &event("revoked", &invitation(), Some(SUBJECT_ID), 3),
        )
        .map(|_record| ())
}

fn verify_create_rollback(repository: &SqliteRepository) {
    let connection = repository.test_connection().expect("connection");
    assert_eq!(
        count(&connection, "yard_guest_invitations", "id", INVITATION_ID),
        0
    );
    assert_eq!(
        count(&connection, "yard_access_grants", "id", "yardgrant_guest"),
        0
    );
    assert_eq!(audit_count(&connection, "yard.guest_invite.created"), 0);
}

fn verify_accept_rollback(repository: &SqliteRepository) {
    let (invitation_state, subject_count, key_count, continuation_count, accepted_audit_count) = {
        let connection = repository.test_connection().expect("connection");
        let invitation_state: (String, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT status, token_hash, accepted_subject_id
                 FROM yard_guest_invitations WHERE id = ?1",
                [INVITATION_ID],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("invitation state");
        let result = (
            invitation_state,
            count(&connection, "yard_subjects", "id", SUBJECT_ID),
            count(
                &connection,
                "yard_guest_login_keys",
                "secret_hash",
                KEY_HASH,
            ),
            count(
                &connection,
                "yard_continuations",
                "id",
                "continuation_guest",
            ),
            audit_count(&connection, "yard.guest_invite.accepted"),
        );
        drop(connection);
        result
    };
    assert_eq!(
        invitation_state,
        ("pending".to_owned(), Some(TOKEN_HASH.to_owned()), None)
    );
    assert_eq!(subject_count, 0);
    assert_eq!(key_count, 0);
    assert_eq!(continuation_count, 0);
    assert_eq!(accepted_audit_count, 0);
}

fn verify_revoke_rollback(repository: &SqliteRepository) {
    let connection = repository.test_connection().expect("connection");
    let invitation_state: (String, Option<i64>) = connection
        .query_row(
            "SELECT status, revoked_at_ms FROM yard_guest_invitations WHERE id = ?1",
            [INVITATION_ID],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("invitation state");
    assert_eq!(invitation_state, ("accepted".to_owned(), None));
    let grant_state: (String, Option<i64>) = connection
        .query_row(
            "SELECT status, revoked_at_ms FROM yard_access_grants WHERE id = 'yardgrant_guest'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("grant state");
    assert_eq!(grant_state, ("active".to_owned(), None));
    let key_revoked_at: Option<i64> = connection
        .query_row(
            "SELECT revoked_at_ms FROM yard_guest_login_keys WHERE secret_hash = ?1",
            [KEY_HASH],
            |row| row.get(0),
        )
        .expect("key state");
    assert_eq!(key_revoked_at, None);
    assert_eq!(audit_count(&connection, "yard.guest_invite.revoked"), 0);
    drop(connection);
    assert_eq!(
        repository
            .authenticate_yard_guest_key(KEY_HASH, 3)
            .expect("guest key remains active")
            .id,
        SUBJECT_ID
    );
    assert_eq!(
        repository
            .yard_guest_invite_by_id(INVITATION_ID)
            .expect("accepted invitation")
            .status,
        YardGuestInviteStatus::Accepted
    );
    assert!(
        repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3)
            .is_ok()
    );
}

fn count(connection: &rusqlite::Connection, table: &str, field: &str, value: &str) -> i64 {
    connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {field} = ?1"),
            [value],
            |row| row.get(0),
        )
        .expect("row count")
}

fn audit_count(connection: &rusqlite::Connection, action: &str) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action = ?1",
            [action],
            |row| row.get(0),
        )
        .expect("audit count")
}
