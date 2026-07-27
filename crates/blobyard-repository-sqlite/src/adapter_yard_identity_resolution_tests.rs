#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::yard_identity_grants;
use super::yard_identity_test_support::*;
use blobyard_contract::{RepositoryError, YardIdentityRepository, YardManagementRole};

fn insert_identity_group(connection: &rusqlite::Connection) {
    connection
        .execute_batch(
            "INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms)
             VALUES ('group_identity', 'workspace_fixture', 'Identity', 'active', 1, 1, NULL);
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             VALUES ('group_identity', 'workspace_fixture', 'user_identity_fixture', 1);",
        )
        .expect("identity group");
}

#[test]
fn identity_grants_include_groups_and_fail_closed_on_corrupt_group_roles() {
    let (_temporary, membership_repository) = repository();
    let connection = membership_repository.test_connection().expect("connection");
    install_policy(&connection, Some("viewer"));
    insert_identity_group(&connection);
    connection
        .execute_batch(
            "INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             VALUES
               ('yardgrant_group_a', 'yard_identity_fixture', NULL, 'group',
                'group_identity', '[\"viewer\"]', 'active', 1, 'fixture', NULL, NULL),
               ('yardgrant_group_b', 'yard_identity_fixture', NULL, 'group',
                'group_identity', '[\"viewer\"]', 'active', 2, 'fixture', NULL, NULL);",
        )
        .expect("group grants");
    let sources = yard_identity_grants::resolve(
        &connection,
        YARD_ID,
        ENVIRONMENT_ID,
        "workspace_fixture",
        USER_ID,
        10,
    )
    .expect("group authority");
    assert_eq!(sources.groups, ["group_identity"]);
    assert_eq!(sources.roles, ["viewer", "viewer"]);

    connection
        .execute(
            "UPDATE yard_access_grants SET app_roles = '{'
             WHERE id = 'yardgrant_group_a'",
            [],
        )
        .expect("corrupt group roles");
    assert_eq!(
        yard_identity_grants::resolve(
            &connection,
            YARD_ID,
            ENVIRONMENT_ID,
            "workspace_fixture",
            USER_ID,
            10,
        )
        .err(),
        Some(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn identity_grants_fail_closed_on_corrupt_direct_roles() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    install_policy(&connection, Some("viewer"));
    connection
        .execute(
            "UPDATE yard_access_grants SET app_roles = '{'
             WHERE id = 'yardgrant_identity'",
            [],
        )
        .expect("corrupt direct roles");
    assert_eq!(
        yard_identity_grants::resolve(
            &connection,
            YARD_ID,
            ENVIRONMENT_ID,
            "workspace_fixture",
            USER_ID,
            10,
        )
        .err(),
        Some(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn identity_grant_rows_reject_non_text_role_storage() {
    for grant_id in ["yardgrant_identity", "yardgrant_group_a"] {
        let (_temporary, repository) = repository();
        let connection = repository.test_connection().expect("connection");
        if grant_id == "yardgrant_group_a" {
            insert_identity_group(&connection);
            connection
                .execute_batch(
                    "INSERT INTO yard_access_grants
                       (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                        status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
                     VALUES
                       ('yardgrant_group_a', 'yard_identity_fixture', NULL, 'group',
                        'group_identity', '[]', 'active', 1, 'fixture', NULL, NULL);",
                )
                .expect("group grant");
        }
        connection
            .execute(
                "UPDATE yard_access_grants SET app_roles = 1 WHERE id = ?1",
                [grant_id],
            )
            .expect("non-text roles");
        assert_eq!(
            yard_identity_grants::resolve(
                &connection,
                YARD_ID,
                ENVIRONMENT_ID,
                "workspace_fixture",
                USER_ID,
                10,
            )
            .err(),
            Some(RepositoryError::Unavailable),
            "{grant_id}"
        );
        drop(connection);
    }
}

#[test]
fn identity_grants_propagate_missing_membership_and_grant_storage() {
    let (_temporary, grant_repository) = repository();
    let connection = grant_repository.test_connection().expect("connection");
    connection
        .execute_batch("DROP TABLE workspace_group_members")
        .expect("remove membership storage");
    assert_eq!(
        yard_identity_grants::resolve(
            &connection,
            YARD_ID,
            ENVIRONMENT_ID,
            "workspace_fixture",
            USER_ID,
            10,
        )
        .err(),
        Some(RepositoryError::Unavailable)
    );
    drop(connection);

    let (_temporary, missing_grant_repository) = repository();
    let connection = missing_grant_repository
        .test_connection()
        .expect("connection");
    connection
        .execute_batch("DROP TABLE yard_access_grants")
        .expect("remove grant storage");
    assert_eq!(
        yard_identity_grants::resolve(
            &connection,
            YARD_ID,
            ENVIRONMENT_ID,
            "workspace_fixture",
            USER_ID,
            10,
        )
        .err(),
        Some(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn identity_grants_reject_malformed_nonstrict_rows() {
    let (_temporary, nonstrict_repository) = repository();
    let connection = nonstrict_repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch(
            "DROP TABLE yard_access_grants;
             CREATE TABLE yard_access_grants (
               id,
               yard_id,
               environment_id,
               principal_kind,
               principal_id,
               app_roles,
               status,
               revoked_at_ms,
               expires_at_ms
             );
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, revoked_at_ms, expires_at_ms)
             VALUES
               ('grant_bad_roles', 'yard_identity_fixture', NULL, 'user',
                'user_identity_fixture', 1, 'active', NULL, NULL);",
        )
        .expect("non-strict grant storage");
    assert_eq!(
        yard_identity_grants::resolve(
            &connection,
            YARD_ID,
            ENVIRONMENT_ID,
            "workspace_fixture",
            USER_ID,
            10,
        )
        .err(),
        Some(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn identity_grants_validate_inputs() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    for values in [
        ["", ENVIRONMENT_ID, "workspace_fixture", USER_ID],
        [YARD_ID, "", "workspace_fixture", USER_ID],
        [YARD_ID, ENVIRONMENT_ID, "", USER_ID],
        [YARD_ID, ENVIRONMENT_ID, "workspace_fixture", ""],
    ] {
        assert_eq!(
            yard_identity_grants::resolve(
                &connection,
                values[0],
                values[1],
                values[2],
                values[3],
                10,
            )
            .err(),
            Some(RepositoryError::InvalidInput)
        );
    }
    drop(connection);
}

include!("adapter_yard_identity_group_bounds_tests.rs");
include!("adapter_yard_identity_runtime_tests.rs");
