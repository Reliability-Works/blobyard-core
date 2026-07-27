fn assert_poisoned_yard_identity(repository: &SqliteRepository) {
    let event = NewAuditEvent {
        id: "audit_identity_failure".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "fixture".to_owned(),
        request_id: "request_identity_failure".to_owned(),
        target_type: "fixture".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    };
    unavailable(repository.list_yard_management_roles("yard_fixture", None));
    unavailable(repository.set_yard_management_role(
        "yard_fixture",
        "user_fixture",
        YardManagementRole::Owner,
        1,
        &event,
    ));
    unavailable(repository.revoke_yard_management_role("yard_fixture", "user_fixture", 1, &event));
    unavailable(repository.get_yard_application_policy("yard_fixture"));
    unavailable(repository.set_yard_application_policy(
        "yard_fixture",
        &checksum('a'),
        blobyard_testkit::yard_application_policy(),
        "fixture",
        1,
        &event,
    ));
    unavailable(repository.set_yard_access_roles("yard_fixture", "grant_fixture", &[], 1, &event));
    unavailable(repository.resolve_yard_identity("fixture-host", &checksum('b'), 1));
}
