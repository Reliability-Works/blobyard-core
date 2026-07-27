#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::events::hash;
use super::tests::{Fixture, INVITATION_ID, SUBJECT_ID, session};
use blobyard_contract::{
    RepositoryError, YardIdentityRepository, YardSessionAuditContext, YardSessionRepository,
};

#[test]
fn guest_identity_rejects_invalid_lookup_input_and_role_encoding() {
    assert_lookup_failures();
    assert_corrupt_role_encoding();
}

fn assert_lookup_failures() {
    let connection = rusqlite::Connection::open_in_memory().expect("connection");
    assert!(matches!(
        super::super::yard_guest_identity::resolve(
            &connection,
            "",
            "environment",
            "workspace",
            "subject",
            "invitation",
            1,
        ),
        Err(RepositoryError::InvalidInput)
    ));
    assert!(matches!(
        super::super::yard_guest_identity::resolve(
            &connection,
            "yard",
            "environment",
            "workspace",
            "subject",
            "invitation",
            1,
        ),
        Err(RepositoryError::Unavailable)
    ));
}

fn assert_corrupt_role_encoding() {
    let fixture = Fixture::new();
    fixture.create();
    fixture.accept();
    let exchange = fixture
        .repository
        .exchange_yard_session_code(
            &hash('f'),
            "guest-yard-fixture",
            &session(),
            &YardSessionAuditContext {
                id: "audit_guest_corrupt_roles".to_owned(),
                request_id: "request_guest_corrupt_roles".to_owned(),
            },
            3,
        )
        .expect("exchange");
    let connection = fixture.repository.test_connection().expect("connection");
    connection
        .execute(
            "UPDATE yard_access_grants SET app_roles = '{'
             WHERE id = 'yardgrant_guest'",
            [],
        )
        .expect("corrupt roles");
    assert!(matches!(
        super::super::yard_guest_identity::resolve(
            &connection,
            "yard_guest",
            "environment_guest",
            "workspace_guest",
            SUBJECT_ID,
            INVITATION_ID,
            3,
        ),
        Err(RepositoryError::Unavailable)
    ));
    drop(connection);
    assert_eq!(
        fixture.repository.resolve_yard_identity(
            "guest-yard-fixture",
            &exchange.session.token_hash,
            4,
        ),
        Err(RepositoryError::Unavailable)
    );
}
