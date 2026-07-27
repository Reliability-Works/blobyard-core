fn assert_viewer_role_update_unavailable(transaction: &rusqlite::Transaction<'_>) {
    assert_eq!(
        yard_application_policy::set_grant_roles(
            transaction,
            YARD_ID,
            "yardgrant_identity",
            &["viewer".to_owned()],
            10,
            &access_roles_event(&[], &["viewer"], 10),
        ),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn nonempty_access_roles_require_an_approved_policy() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    assert_eq!(
        yard_application_policy::validated_roles(&connection, YARD_ID, &["viewer".to_owned()]),
        Err(RepositoryError::InvalidInput)
    );
    drop(connection);
}

#[test]
fn access_role_updates_map_each_write_failure() {
    for (trigger, expected) in [
        (
            "CREATE TRIGGER fail_access_role_update
             BEFORE UPDATE OF app_roles ON yard_access_grants
             BEGIN DELETE FROM missing_access_role_storage; END;",
            RepositoryError::Unavailable,
        ),
        (
            "CREATE TRIGGER ignore_access_role_update
             BEFORE UPDATE OF app_roles ON yard_access_grants
             BEGIN SELECT RAISE(IGNORE); END;",
            RepositoryError::Conflict,
        ),
    ] {
        let (_temporary, repository) = repository();
        let mut connection = repository.test_connection().expect("connection");
        install_policy(&connection, None);
        connection
            .execute_batch(trigger)
            .expect("access role trigger");
        let transaction = connection.transaction().expect("transaction");
        assert_eq!(
            yard_application_policy::set_grant_roles(
                &transaction,
                YARD_ID,
                "yardgrant_identity",
                &["viewer".to_owned()],
                10,
                &access_roles_event(&[], &["viewer"], 10),
            ),
            Err(expected)
        );
        drop(transaction);
        drop(connection);
    }

    let (_temporary, lookup_repository) = repository();
    let mut connection = lookup_repository.test_connection().expect("connection");
    install_policy(&connection, None);
    connection
        .execute_batch("DROP TABLE audit_events")
        .expect("remove audit storage");
    let transaction = connection.transaction().expect("transaction");
    assert_viewer_role_update_unavailable(&transaction);
    drop(transaction);
    drop(connection);
}

#[test]
fn access_role_updates_propagate_lookup_and_validation_failures() {
    let (_temporary, validation_repository) = repository();
    let mut connection = validation_repository.test_connection().expect("connection");
    connection
        .execute_batch("DROP TABLE yard_access_grants")
        .expect("remove grant storage");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "yardgrant_identity",
            &[],
            10,
            &access_roles_event(&[], &[], 10),
        ),
        Err(RepositoryError::Unavailable)
    );
    drop(transaction);
    drop(connection);

    let (_temporary, role_repository) = repository();
    let mut connection = role_repository.test_connection().expect("connection");
    install_policy(&connection, None);
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        yard_application_policy::set_grant_roles(
            &transaction,
            YARD_ID,
            "yardgrant_identity",
            &["unknown".to_owned()],
            10,
            &access_roles_event(&[], &["unknown"], 10),
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn access_role_updates_propagate_reread_failures() {
    for trigger in [
        "CREATE TRIGGER remove_access_role_grant
         AFTER UPDATE OF app_roles ON yard_access_grants
         BEGIN DELETE FROM yard_access_grants WHERE id = NEW.id; END;",
        "CREATE TRIGGER corrupt_access_role_grant
         AFTER UPDATE OF app_roles ON yard_access_grants
         BEGIN UPDATE yard_access_grants SET app_roles = '{' WHERE id = NEW.id; END;",
    ] {
        let (_temporary, repository) = repository();
        let mut connection = repository.test_connection().expect("connection");
        install_policy(&connection, None);
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .expect("ignore grant checks");
        connection.execute_batch(trigger).expect("reread trigger");
        let transaction = connection.transaction().expect("transaction");
        assert_viewer_role_update_unavailable(&transaction);
        drop(transaction);
        drop(connection);
    }
}
