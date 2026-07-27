fn seed_management_role_transitions(transaction: &rusqlite::Transaction<'_>) {
    for (user_id, from, to, at) in [
        (USER_ID, None, YardManagementRole::Owner, 2),
        (
            USER_ID,
            Some(YardManagementRole::Owner),
            YardManagementRole::Owner,
            3,
        ),
        ("user_backup_owner", None, YardManagementRole::Owner, 4),
        (
            "user_backup_owner",
            Some(YardManagementRole::Owner),
            YardManagementRole::Admin,
            5,
        ),
        (
            "user_backup_owner",
            Some(YardManagementRole::Admin),
            YardManagementRole::Developer,
            6,
        ),
    ] {
        assert!(set_role(transaction, user_id, from, to, at).is_ok());
    }
}

#[test]
fn management_role_transitions_cover_assignment_state_changes() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        set_role(&transaction, USER_ID, None, YardManagementRole::Admin, 1),
        Err(RepositoryError::Conflict)
    );
    seed_management_role_transitions(&transaction);
    assert_eq!(
        set_role(
            &transaction,
            USER_ID,
            Some(YardManagementRole::Owner),
            YardManagementRole::Admin,
            7,
        ),
        Err(RepositoryError::Conflict)
    );
    drop(transaction);
    drop(connection);
}

#[test]
fn management_role_transitions_cover_revocation_states() {
    let (_temporary, repository) = repository();
    let mut connection = repository.test_connection().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    seed_management_role_transitions(&transaction);
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            "user_backup_owner",
            8,
            &role_event(
                "yard.management_role_revoked",
                "user_backup_owner",
                Some(YardManagementRole::Developer),
                None,
                8,
            ),
        ),
        Ok(())
    );
    assert_eq!(
        yard_management_roles::revoke(
            &transaction,
            YARD_ID,
            USER_ID,
            9,
            &role_event(
                "yard.management_role_revoked",
                USER_ID,
                Some(YardManagementRole::Owner),
                None,
                9,
            ),
        ),
        Err(RepositoryError::Conflict)
    );
    drop(transaction);
    drop(connection);
}
