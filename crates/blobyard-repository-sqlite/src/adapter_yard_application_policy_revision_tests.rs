#[test]
fn application_policy_rejects_unrepresentable_next_revision() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    insert_owner(&connection, USER_ID);
    install_policy(&connection, None);
    connection
        .execute(
            "UPDATE yard_application_policies SET revision = ?1 WHERE yard_id = ?2",
            params![i64::MAX, YARD_ID],
        )
        .expect("maximum revision");
    drop(connection);
    let digest = "c".repeat(64);
    let event = revision_overflow_event(&digest);
    assert_eq!(
        repository.set_yard_application_policy(
            YARD_ID,
            &digest,
            graph(None),
            "fixture",
            10,
            &event,
        ),
        Err(RepositoryError::Conflict)
    );
}

fn revision_overflow_event(digest: &str) -> blobyard_contract::NewAuditEvent {
    let mut metadata = vec![
        (
            "fromRevision".to_owned(),
            blobyard_contract::AuditValue::Number(i64::MAX as u64),
        ),
        (
            "permissionCount".to_owned(),
            blobyard_contract::AuditValue::Number(1),
        ),
        (
            "roleCount".to_owned(),
            blobyard_contract::AuditValue::Number(1),
        ),
        (
            "sourceManifestDigest".to_owned(),
            blobyard_contract::AuditValue::String(digest.to_owned()),
        ),
        (
            "toRevision".to_owned(),
            blobyard_contract::AuditValue::Number((i64::MAX as u64) + 1),
        ),
        (
            "yardId".to_owned(),
            blobyard_contract::AuditValue::String(YARD_ID.to_owned()),
        ),
    ];
    metadata.sort_by(|left, right| left.0.cmp(&right.0));
    blobyard_contract::NewAuditEvent {
        id: "audit_policy_revision_overflow".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "yard.application_policy_set".to_owned(),
        request_id: "request_policy_revision_overflow".to_owned(),
        target_type: "yard_application_policy".to_owned(),
        metadata,
        created_at_ms: 10,
    }
}
