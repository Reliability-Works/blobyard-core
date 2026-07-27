#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::yard_management_roles;
use super::yard_identity_test_support::*;
use blobyard_contract::{
    AuditValue, NewAuditEvent, RepositoryError, YardManagementRole, YardManagementRoleCursor,
};
use rusqlite::{Connection, params};

fn role_event(
    action: &str,
    user_id: &str,
    from: Option<YardManagementRole>,
    to: Option<YardManagementRole>,
    at: u64,
) -> NewAuditEvent {
    let mut metadata = vec![
        (
            "from".to_owned(),
            from.map_or(AuditValue::Null, |role| {
                AuditValue::String(role.as_str().to_owned())
            }),
        ),
        ("userId".to_owned(), AuditValue::String(user_id.to_owned())),
        ("yardId".to_owned(), AuditValue::String(YARD_ID.to_owned())),
    ];
    if let Some(role) = to {
        metadata.push((
            "to".to_owned(),
            AuditValue::String(role.as_str().to_owned()),
        ));
    }
    metadata.sort_by(|left, right| left.0.cmp(&right.0));
    NewAuditEvent {
        id: format!("audit_{action}_{user_id}_{at}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}_{user_id}_{at}"),
        target_type: "yard_management_role".to_owned(),
        metadata,
        created_at_ms: at,
    }
}

fn insert_assignment(connection: &Connection, user_id: &str, role: YardManagementRole) {
    connection
        .execute(
            "INSERT INTO yard_management_role_assignments
               (yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 'workspace_fixture', ?3, 1, 1)",
            params![YARD_ID, user_id, role.as_str()],
        )
        .expect("role assignment");
}

fn set_role(
    transaction: &rusqlite::Transaction<'_>,
    user_id: &str,
    from: Option<YardManagementRole>,
    to: YardManagementRole,
    at: i64,
) -> Result<(), RepositoryError> {
    yard_management_roles::set(
        transaction,
        YARD_ID,
        user_id,
        to,
        at,
        &role_event(
            "yard.management_role_set",
            user_id,
            from,
            Some(to),
            u64::try_from(at).expect("timestamp"),
        ),
    )
    .map(|_assignment| ())
}

#[test]
fn management_role_sets_reject_invalid_users_and_times() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_management_roles::set(
            &transaction,
            YARD_ID,
            "",
            YardManagementRole::Auditor,
            10,
            &role_event(
                "yard.management_role_set",
                "",
                None,
                Some(YardManagementRole::Auditor),
                10,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_management_roles::set(
            &transaction,
            YARD_ID,
            "user_missing",
            YardManagementRole::Auditor,
            10,
            &role_event(
                "yard.management_role_set",
                "user_missing",
                None,
                Some(YardManagementRole::Auditor),
                10,
            ),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        yard_management_roles::set(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            YardManagementRole::Auditor,
            -1,
            &role_event(
                "yard.management_role_set",
                "user_backup_owner",
                None,
                Some(YardManagementRole::Auditor),
                0,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn management_role_revokes_reject_invalid_users_times_and_yards() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    insert_assignment(
        &connection,
        "user_backup_owner",
        YardManagementRole::Auditor,
    );
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            "user_missing",
            10,
            &role_event(
                "yard.management_role_revoked",
                "user_missing",
                None,
                None,
                10,
            ),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            -1,
            &role_event(
                "yard.management_role_revoked",
                "user_backup_owner",
                Some(YardManagementRole::Auditor),
                None,
                0,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            "yard_identity_inactive",
            "user_backup_owner",
            10,
            &role_event(
                "yard.management_role_revoked",
                "user_backup_owner",
                Some(YardManagementRole::Auditor),
                None,
                10,
            ),
        ),
        Err(RepositoryError::NotFound)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn management_role_mutations_reject_ownerless_state() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    insert_assignment(
        &connection,
        "user_backup_owner",
        YardManagementRole::Auditor,
    );
    let transaction = connection.transaction().expect("transaction");
    transaction
        .execute(
            "DELETE FROM yard_management_role_assignments WHERE role = 'owner'",
            [],
        )
        .expect("remove all owners");
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            10,
            &role_event(
                "yard.management_role_revoked",
                "user_backup_owner",
                Some(YardManagementRole::Auditor),
                None,
                10,
            ),
        ),
        Err(RepositoryError::Unavailable)
    );
    drop(transaction);
    drop(connection);
}

include!("adapter_yard_identity_transition_tests.rs");
include!("adapter_yard_management_role_fault_tests.rs");
