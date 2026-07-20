use blobyard_contract::{AuditValue, NewAuditEvent};

pub(super) fn capability_event(
    action: &str,
    target_type: &str,
    target_key: &str,
    target_id: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{created_at_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{created_at_ms}"),
        target_type: target_type.to_owned(),
        metadata: vec![(
            target_key.to_owned(),
            AuditValue::String(target_id.to_owned()),
        )],
        created_at_ms,
    }
}
