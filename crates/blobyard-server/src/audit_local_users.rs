pub(crate) fn local_user_event(
    principal: &blobyard_contract::LocalApiTokenRecord,
    action: &str,
    user_id: &str,
    created_at_ms: u64,
) -> blobyard_contract::NewAuditEvent {
    action_event(
        principal,
        action,
        "local_user",
        vec![(
            "userId".to_owned(),
            blobyard_contract::AuditValue::String(user_id.to_owned()),
        )],
        created_at_ms,
    )
}
