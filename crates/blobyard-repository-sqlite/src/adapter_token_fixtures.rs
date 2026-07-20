use super::checksum;
use blobyard_contract::{AuditValue, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent};

pub(super) fn session(token: &LocalApiTokenRecord) -> LocalCliSessionRecord {
    blobyard_testkit::cli_session_record(token, "0.1.12")
}

pub(super) fn token_audit(action: &str, token_id: &str, created_at_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{token_id}_{created_at_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "token_fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{token_id}_{created_at_ms}"),
        target_type: "api_token".to_owned(),
        metadata: vec![(
            "tokenId".to_owned(),
            AuditValue::String(token_id.to_owned()),
        )],
        created_at_ms,
    }
}

pub(super) fn token() -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash: checksum('c'),
        scopes: vec!["object:read".to_owned()],
        workspace_id: "workspace_fixture".to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 1_000,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}
