#[test]
fn management_role_rows_fail_closed_on_corrupt_values() {
    for statement in [
        "UPDATE yard_management_role_assignments SET role = 'invalid'
         WHERE user_id = 'user_backup_owner'",
        "UPDATE yard_management_role_assignments SET updated_at_ms = 0
         WHERE user_id = 'user_backup_owner'",
        "UPDATE yard_management_role_assignments SET user_id = ''
         WHERE user_id = 'user_backup_owner'",
        "UPDATE yard_management_role_assignments SET workspace_id = ''
         WHERE user_id = 'user_backup_owner'",
        "UPDATE yard_management_role_assignments SET created_at_ms = -1
         WHERE user_id = 'user_backup_owner'",
        "UPDATE yard_management_role_assignments SET role = 1
         WHERE user_id = 'user_backup_owner'",
    ] {
        let (_temporary, repository) = repository();
        let connection = repository.test_connection().expect("connection");
        insert_owner(&connection, USER_ID);
        insert_assignment(
            &connection,
            "user_backup_owner",
            YardManagementRole::Auditor,
        );
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys");
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .expect("ignore role checks");
        connection.execute(statement, []).expect("corrupt role row");
        assert_eq!(
            yard_management_roles::list(&connection, YARD_ID, None),
            Err(RepositoryError::Unavailable)
        );
        drop(connection);
    }
}

#[test]
fn management_role_state_and_active_yard_query_failures_are_stable() {
    let (_temporary, state_repository) = repository();
    let connection = state_repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch("DROP TABLE yard_management_role_assignments")
        .expect("remove role state");
    assert_eq!(
        yard_management_roles::list(&connection, YARD_ID, None),
        Err(RepositoryError::Unavailable)
    );
    drop(connection);

    let (_temporary, yard_repository) = repository();
    let connection = yard_repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch("DROP TABLE web_yards")
        .expect("remove yard state");
    assert_eq!(
        yard_management_roles::list(&connection, YARD_ID, None),
        Err(RepositoryError::Unavailable)
    );
    drop(connection);
}

#[test]
fn management_role_revoke_maps_delete_failures_and_noops() {
    for (trigger, expected) in [
        (
            "CREATE TRIGGER role_revoke_failure
             BEFORE DELETE ON yard_management_role_assignments
             BEGIN DELETE FROM missing_role_storage; END;",
            RepositoryError::Unavailable,
        ),
        (
            "CREATE TRIGGER role_revoke_ignore
             BEFORE DELETE ON yard_management_role_assignments
             BEGIN SELECT RAISE(IGNORE); END;",
            RepositoryError::Conflict,
        ),
    ] {
        let (_temporary, repository) = repository();
        let mut connection = repository.test_connection().expect("connection");
        insert_owner(&connection, USER_ID);
        insert_assignment(
            &connection,
            "user_backup_owner",
            YardManagementRole::Auditor,
        );
        connection.execute_batch(trigger).expect("revoke trigger");
        let transaction = connection.transaction().expect("transaction");
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
            Err(expected)
        );
        drop(transaction);
        drop(connection);
    }
}

#[test]
fn management_role_revoke_propagates_target_lookup_failure() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch(
            "DROP TABLE yard_management_role_assignments;
             CREATE TABLE yard_management_role_assignments (
               yard_id TEXT NOT NULL,
               role TEXT NOT NULL
             );
             INSERT INTO yard_management_role_assignments (yard_id, role)
             VALUES ('yard_identity_fixture', 'owner');",
        )
        .expect("incomplete role storage");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            USER_ID,
            10,
            &role_event(
                "yard.management_role_revoked",
                USER_ID,
                Some(YardManagementRole::Owner),
                None,
                10,
            ),
        ),
        Err(RepositoryError::Unavailable)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn management_role_list_validates_yards_and_storage_shape() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    assert_eq!(
        yard_management_roles::list(&connection, "", None),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_management_roles::list(&connection, "yard_missing", None),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        yard_management_roles::list(
            &connection,
            YARD_ID,
            Some(&YardManagementRoleCursor {
                role: YardManagementRole::Owner,
                user_id: String::new(),
            }),
        ),
        Err(RepositoryError::InvalidInput)
    );
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch(
            "DROP TABLE yard_management_role_assignments;
             CREATE TABLE yard_management_role_assignments (
               yard_id TEXT NOT NULL,
               role TEXT NOT NULL
             );
             INSERT INTO yard_management_role_assignments (yard_id, role)
             VALUES ('yard_identity_fixture', 'owner');",
        )
        .expect("incomplete role storage");
    assert_eq!(
        yard_management_roles::list(&connection, YARD_ID, None),
        Err(RepositoryError::Unavailable)
    );
    drop(connection);
}
