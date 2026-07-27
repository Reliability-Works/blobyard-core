#[test]
fn identity_public_operations_reject_unrepresentable_times_and_credentials() {
    let (_temporary, repository) = repository();
    let event = bad_event("fixture", "fixture");
    assert_eq!(
        repository.set_yard_management_role(
            YARD_ID,
            USER_ID,
            YardManagementRole::Owner,
            u64::MAX,
            &event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_yard_management_role(YARD_ID, USER_ID, u64::MAX, &event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            &"c".repeat(64),
            graph(None),
            "fixture",
            u64::MAX,
            &event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.set_yard_access_roles(YARD_ID, "yardgrant_identity", &[], u64::MAX, &event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.resolve_yard_identity(HOST, TOKEN_HASH, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.resolve_yard_identity("", TOKEN_HASH, 10),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.resolve_yard_identity(HOST, "", 10),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn management_role_pagination_limits_are_enforced() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    for index in 0..51 {
        let user_id = format!("user_page_{index:03}");
        connection
            .execute(
                "INSERT INTO local_users
                   (id, workspace_id, display_name, email, status, created_at_ms,
                    deactivated_at_ms)
                 VALUES (?1, 'workspace_fixture', ?1, NULL, 'active', 1, NULL)",
                [&user_id],
            )
            .expect("page user");
        connection
            .execute(
                "INSERT INTO yard_management_role_assignments
                   (yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, 'workspace_fixture', 'auditor', 1, 1)",
                params![YARD_ID, user_id],
            )
            .expect("page assignment");
    }
    let first = yard_management_roles::list(&connection, YARD_ID, None).expect("first page");
    assert_eq!(first.items.len(), 50);
    let cursor = first.next_cursor.expect("next cursor");
    let second =
        yard_management_roles::list(&connection, YARD_ID, Some(&cursor)).expect("second page");
    assert_eq!(second.items.len(), 2);
    assert_eq!(
        yard_management_roles::list(
            &connection,
            YARD_ID,
            Some(&YardManagementRoleCursor {
                role: YardManagementRole::Auditor,
                user_id: String::new(),
            })
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(connection);
}

#[test]
fn management_role_audit_validation_is_enforced() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    insert_owner(&connection, "user_backup_owner");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_management_roles::set(
            &transaction,
            YARD_ID,
            USER_ID,
            YardManagementRole::Owner,
            10,
            &bad_event("yard.management_role_set", "yard_management_role"),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            10,
            &bad_event("yard.management_role_revoked", "yard_management_role"),
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn management_role_assignment_limit_rejects_the_next_user() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    for index in 1..MAXIMUM_YARD_MANAGEMENT_ROLES {
        let user_id = format!("user_limit_{index:03}");
        connection
            .execute(
                "INSERT INTO local_users
                   (id, workspace_id, display_name, email, status, created_at_ms,
                    deactivated_at_ms)
                 VALUES (?1, 'workspace_fixture', ?1, NULL, 'active', 1, NULL)",
                [&user_id],
            )
            .expect("limit user");
        connection
            .execute(
                "INSERT INTO yard_management_role_assignments
                   (yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, 'workspace_fixture', 'auditor', 1, 1)",
                params![YARD_ID, user_id],
            )
            .expect("limit assignment");
    }
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_management_roles::set(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            YardManagementRole::Auditor,
            10,
            &bad_event("yard.management_role_set", "yard_management_role"),
        ),
        Err(RepositoryError::Conflict)
    );
    drop(transaction);
    drop(connection);
}
