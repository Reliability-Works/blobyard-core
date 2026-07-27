#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::{yard_application_policy, yard_management_roles};
use super::yard_identity_test_support::*;
use blobyard_contract::{
    MAXIMUM_YARD_MANAGEMENT_ROLES, RepositoryError, YardIdentityRepository, YardManagementRole,
    YardManagementRoleCursor,
};
use rusqlite::params;

#[test]
fn application_policy_rejects_inactive_cross_scope_and_corrupt_records() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    assert_eq!(
        yard_application_policy::get(&connection, "yard_identity_inactive"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        yard_application_policy::get(&connection, ""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(yard_application_policy::get(&connection, YARD_ID), Ok(None));
    install_policy(&connection, None);
    connection
        .pragma_update(None, "ignore_check_constraints", true)
        .expect("ignore policy checks");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute(
            "UPDATE yard_application_policies SET workspace_id = 'workspace_other'
             WHERE yard_id = ?1",
            [YARD_ID],
        )
        .expect("cross-scope policy");
    assert_eq!(
        yard_application_policy::get(&connection, YARD_ID),
        Err(RepositoryError::Unavailable)
    );
    connection
        .execute(
            "UPDATE yard_application_policies
             SET workspace_id = 'workspace_fixture', effective_json = '{}'
             WHERE yard_id = ?1",
            [YARD_ID],
        )
        .expect("corrupt policy");
    assert_eq!(
        yard_application_policy::policy_by_yard(&connection, YARD_ID),
        Err(RepositoryError::Unavailable)
    );
    install_policy(&connection, None);
    connection
        .execute(
            "UPDATE yard_application_policies SET revision = 0 WHERE yard_id = ?1",
            [YARD_ID],
        )
        .expect("zero policy revision");
    assert_eq!(
        yard_application_policy::policy_by_yard(&connection, YARD_ID),
        Err(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn application_policy_get_propagates_policy_storage_failure() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch("DROP TABLE yard_application_policies")
        .expect("remove policy storage");
    assert_eq!(
        yard_application_policy::get(&connection, YARD_ID),
        Err(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn application_policy_rows_reject_each_corrupt_storage_shape() {
    for statement in [
        "UPDATE yard_application_policies SET source_manifest_digest = 'invalid'",
        "UPDATE yard_application_policies SET policy_json = '{'",
        "UPDATE yard_application_policies SET effective_json = '{'",
        "UPDATE yard_application_policies SET approved_at_ms = -1",
    ] {
        let (_temporary, repository) = repository();
        let connection = repository.test_connection().expect("connection");
        install_policy(&connection, None);
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys");
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .expect("ignore policy checks");
        connection
            .execute(statement, [])
            .expect("corrupt policy row");
        assert_eq!(
            yard_application_policy::policy_by_yard(&connection, YARD_ID),
            Err(RepositoryError::Unavailable),
            "{statement}"
        );
        drop(connection);
    }
}

#[test]
fn policy_mutations_reject_invalid_contracts_before_writing() {
    let (_temporary, repository) = repository();
    {
        let connection = repository.test_connection().expect("connection");
        insert_owner(&connection, USER_ID);
        install_policy(&connection, None);
    }
    let policy_event = blobyard_testkit::yard_policy_event(YARD_ID, &"c".repeat(64), 10);
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            "not-a-digest",
            graph(None),
            "fixture",
            10,
            &policy_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            &"c".repeat(64),
            graph(None),
            "",
            10,
            &policy_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid_graph = graph(None);
    invalid_graph.default_role = Some("missing".to_owned());
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            &"c".repeat(64),
            invalid_graph,
            "fixture",
            10,
            &policy_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            &"c".repeat(64),
            graph(None),
            "fixture",
            10,
            &bad_event("yard.application_policy_set", "yard_application_policy"),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn policy_role_and_timestamp_validation_fail_before_writing() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    install_policy(&connection, None);
    let policy_event = blobyard_testkit::yard_policy_event(YARD_ID, &"c".repeat(64), 10);
    assert_eq!(
        yard_application_policy::validated_roles(&connection, YARD_ID, &["unknown".to_owned()]),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_application_policy::validated_roles(
            &connection,
            YARD_ID,
            &["viewer".to_owned(), "viewer".to_owned()],
        ),
        Err(RepositoryError::InvalidInput)
    );
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_application_policy::set(
            &transaction,
            YARD_ID,
            &"c".repeat(64),
            graph(None),
            "fixture",
            -1,
            &policy_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn grant_role_mutations_validate_lookup_time_and_yard_state() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    install_policy(&connection, None);
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "grant_missing",
            &[],
            10,
            &bad_event("yard.access_roles_set", "yard_access_grant"),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "yardgrant_identity",
            &[],
            -1,
            &bad_event("yard.access_roles_set", "yard_access_grant"),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            "yard_identity_inactive",
            "yardgrant_identity",
            &[],
            10,
            &bad_event("yard.access_roles_set", "yard_access_grant"),
        ),
        Err(RepositoryError::NotFound)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn grant_role_mutations_reject_revoked_grants_and_invalid_audits() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    install_policy(&connection, None);
    let transaction = connection.transaction().expect("transaction");
    transaction
        .execute(
            "UPDATE yard_access_grants
             SET status = 'revoked', revoked_at_ms = 9
             WHERE id = 'yardgrant_identity'",
            [],
        )
        .expect("revoke grant");
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "yardgrant_identity",
            &[],
            10,
            &bad_event("yard.access_roles_set", "yard_access_grant"),
        ),
        Err(RepositoryError::NotFound)
    );
    transaction
        .execute(
            "UPDATE yard_access_grants
             SET status = 'active', revoked_at_ms = NULL
             WHERE id = 'yardgrant_identity'",
            [],
        )
        .expect("restore grant");
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "yardgrant_identity",
            &["viewer".to_owned()],
            10,
            &bad_event("yard.access_roles_set", "yard_access_grant"),
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

include!("adapter_yard_application_policy_revision_tests.rs");
include!("adapter_yard_identity_access_role_tests.rs");
include!("adapter_yard_identity_management_edge_tests.rs");
