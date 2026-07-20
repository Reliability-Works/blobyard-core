#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{checksum, session, stable_behavior::repository, token, token_audit};
use blobyard_contract::{
    CredentialRepository, LifecycleRepository, NewAuditEvent, RepositoryError,
};

#[test]
fn api_token_mutations_and_audit_events_commit_atomically() {
    let (_temporary, repository) = repository();
    let token = token();
    let create_collision = token_audit("api_token.created", &token.id, token.created_at_ms);
    repository
        .record_audit(&create_collision)
        .expect("create audit collision");
    assert_eq!(
        repository.create_api_token(&token, &create_collision),
        Err(RepositoryError::Conflict)
    );
    assert!(repository.list_api_tokens().expect("tokens").is_empty());

    let mut create_event = create_collision;
    create_event.id = "audit_token_created_fresh".to_owned();
    repository
        .create_api_token(&token, &create_event)
        .expect("token and create audit");
    let revoke_collision = token_audit("api_token.revoked", &token.id, 2);
    repository
        .record_audit(&revoke_collision)
        .expect("revoke audit collision");
    assert_eq!(
        repository.revoke_api_token(&token.id, 2, &revoke_collision),
        Err(RepositoryError::Conflict)
    );
    assert!(
        repository
            .authenticate_api_token(&token.secret_hash, 2)
            .is_ok()
    );

    let revoke_event = token_audit("api_token.revoked", &token.id, 3);
    repository
        .revoke_api_token(&token.id, 3, &revoke_event)
        .expect("revocation and audit");
    let tokens = repository.list_api_tokens().expect("tokens");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].revoked_at_ms, Some(3));
    let audit = repository
        .list_audit("workspace_fixture", None, 10)
        .expect("audit");
    assert_eq!(audit.items.len(), 4);
    assert_eq!(audit.items[0].action, "api_token.revoked");
    assert_eq!(audit.items[2].action, "api_token.created");
}

#[test]
fn api_token_mutations_reject_every_mismatched_audit_field() {
    let (_temporary, repository) = repository();
    let token = token();
    let create_event = token_audit("api_token.created", &token.id, token.created_at_ms);
    for event in mismatched_events(&create_event) {
        assert_eq!(
            repository.create_api_token(&token, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert!(repository.list_api_tokens().expect("tokens").is_empty());
    repository
        .create_api_token(&token, &create_event)
        .expect("valid token");

    let revoke_event = token_audit("api_token.revoked", &token.id, 2);
    for event in mismatched_events(&revoke_event) {
        assert_eq!(
            repository.revoke_api_token(&token.id, 2, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert!(
        repository
            .authenticate_api_token(&token.secret_hash, 2)
            .is_ok()
    );
}

#[test]
fn api_token_persistence_rejects_each_timestamp_beyond_sqlite_range() {
    let (_temporary, repository) = repository();
    for (index, token) in overflowing_tokens().into_iter().enumerate() {
        let event = token_audit("api_token.created", &token.id, token.created_at_ms);
        assert_eq!(
            repository.create_api_token(&token, &event),
            Err(RepositoryError::InvalidInput),
            "overflow case {index}"
        );
    }
    assert!(repository.list_api_tokens().expect("tokens").is_empty());
}

#[test]
fn cli_session_revocation_rejects_mismatched_audit_and_orphaned_token_state() {
    let (_temporary, repository) = repository();
    let token = token();
    repository
        .install_bootstrap(&checksum('b'))
        .expect("install bootstrap");
    repository
        .exchange_bootstrap(&checksum('b'), &token, &session(&token))
        .expect("exchange bootstrap");
    let event = session_revoke_event(&token.workspace_id, 2);
    for candidate in mismatched_events(&event) {
        assert_eq!(
            repository.revoke_cli_session("session_fixture", &token.workspace_id, 2, &candidate,),
            Err(RepositoryError::InvalidInput)
        );
    }
    repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE api_tokens SET revoked = 1 WHERE id = ?1",
            [&token.id],
        )
        .expect("orphan backing token state");
    assert_eq!(
        repository.revoke_cli_session("session_fixture", &token.workspace_id, 2, &event),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        repository
            .list_cli_sessions(&token.workspace_id)
            .expect("active session")
            .len(),
        1
    );
}

fn session_revoke_event(workspace_id: &str, created_at_ms: u64) -> NewAuditEvent {
    blobyard_testkit::cli_session_revoked_event(workspace_id, "session_fixture", created_at_ms)
}

fn overflowing_tokens() -> Vec<blobyard_contract::LocalApiTokenRecord> {
    let mut values = Vec::new();
    for field in 0..4 {
        let mut value = token();
        match field {
            0 => {
                value.created_at_ms = i64::MAX as u64 + 1;
                value.expires_at_ms = i64::MAX as u64 + 2;
            }
            1 => value.expires_at_ms = u64::MAX,
            2 => value.last_used_at_ms = Some(u64::MAX),
            _ => value.revoked_at_ms = Some(u64::MAX),
        }
        values.push(value);
    }
    values
}

fn mismatched_events(
    event: &blobyard_contract::NewAuditEvent,
) -> Vec<blobyard_contract::NewAuditEvent> {
    let mut events = Vec::new();
    for field in 0..5 {
        let mut candidate = event.clone();
        match field {
            0 => candidate.action.push_str(".wrong"),
            1 => candidate.target_type = "workspace".to_owned(),
            2 => candidate.workspace_id = "workspace_wrong".to_owned(),
            3 => candidate.created_at_ms += 1,
            _ => candidate.metadata.clear(),
        }
        events.push(candidate);
    }
    events
}
