#[test]
fn identity_resolution_applies_defaults_and_rechecks_admission() {
    let (_temporary, repository) = repository();
    {
        let connection = repository.test_connection().expect("connection");
        install_policy(&connection, Some("viewer"));
        insert_owner(&connection, USER_ID);
    }
    let identity = repository
        .resolve_yard_identity(HOST, TOKEN_HASH, 10)
        .expect("identity");
    assert_eq!(identity.management_role, Some(YardManagementRole::Owner));
    assert_eq!(identity.app_roles, ["viewer"]);
    assert_eq!(identity.permissions, ["content.read"]);

    repository
        .test_connection()
        .expect("connection")
        .execute("DELETE FROM yard_access_grants", [])
        .expect("remove admission");
    assert_eq!(
        repository.resolve_yard_identity(HOST, TOKEN_HASH, 11),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn identity_resolution_propagates_admission_storage_failure() {
    let (_temporary, repository) = repository();
    {
        let connection = repository.test_connection().expect("connection");
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys");
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .expect("ignore base checks");
        connection
            .execute_batch("DROP TABLE workspace_groups")
            .expect("remove admission storage");
    }
    assert_eq!(
        repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn identity_resolution_propagates_grant_corruption_and_touch_conflicts() {
    let (_temporary, corrupted_repository) = repository();
    corrupted_repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE yard_access_grants SET app_roles = '{'
             WHERE id = 'yardgrant_identity'",
            [],
        )
        .expect("corrupt direct roles");
    assert_eq!(
        corrupted_repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
        Err(RepositoryError::Unavailable)
    );

    let (_temporary, triggered_repository) = repository();
    triggered_repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER ignore_identity_touch
             BEFORE UPDATE OF last_used_at_ms ON yard_sessions
             BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("touch trigger");
    assert_eq!(
        triggered_repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn identity_resolution_maps_base_row_and_touch_storage_failures() {
    {
        let (_temporary, repository) = repository();
        let connection = repository.test_connection().expect("connection");
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys");
        connection
            .execute_batch("DROP TABLE local_users")
            .expect("remove base identity storage");
        drop(connection);
        assert_eq!(
            repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
            Err(RepositoryError::Unavailable)
        );
    }

    let (_temporary, repository) = repository();
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER fail_identity_touch
             BEFORE UPDATE OF last_used_at_ms ON yard_sessions
             BEGIN DELETE FROM missing_identity_storage; END;",
        )
        .expect("touch failure trigger");
    assert_eq!(
        repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn identity_resolution_propagates_role_and_policy_storage_failures() {
    for statement in [
        "DROP TABLE yard_management_role_assignments",
        "DROP TABLE yard_application_policies",
    ] {
        let (_temporary, repository) = repository();
        let connection = repository.test_connection().expect("connection");
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys");
        connection.execute_batch(statement).expect("remove storage");
        drop(connection);
        assert_eq!(
            repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
            Err(RepositoryError::Unavailable),
            "{statement}"
        );
    }

    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch(
            "DROP TABLE yard_management_role_assignments;
             CREATE TABLE yard_management_role_assignments (
               yard_id TEXT NOT NULL,
               role TEXT NOT NULL
             );",
        )
        .expect("incomplete role lookup storage");
    drop(connection);
    assert_eq!(
        repository.resolve_yard_identity(HOST, TOKEN_HASH, 10),
        Err(RepositoryError::Unavailable)
    );
}
