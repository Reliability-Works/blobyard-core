fn insert_bounded_group_memberships(connection: &rusqlite::Connection) {
    for index in 0..=blobyard_contract::MAXIMUM_USER_GROUPS {
        connection
            .execute(
                "INSERT INTO workspace_groups
                   (id, workspace_id, name, status, member_count, created_at_ms,
                    deactivated_at_ms)
                 VALUES (?1, 'workspace_fixture', ?1, 'active', 1, 1, NULL)",
                [format!("group_bounded_{index:03}")],
            )
            .expect("bounded group");
        connection
            .execute(
                "INSERT INTO workspace_group_members
                   (group_id, workspace_id, user_id, added_at_ms)
                 VALUES (?1, 'workspace_fixture', ?2, 1)",
                rusqlite::params![format!("group_bounded_{index:03}"), USER_ID],
            )
            .expect("bounded membership");
    }
}

#[test]
fn identity_grants_validate_membership_bounds_and_group_storage() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    insert_bounded_group_memberships(&connection);
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
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    connection
        .execute_batch(
            "DELETE FROM workspace_group_members;
             DROP TABLE workspace_groups;",
        )
        .expect("remove group storage");
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
