#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FaultingRepository, Repository};
use crate::transfers::test_seams;
use blobyard_contract::{
    CiAction, CiRepository, LocalCiTrustRecord, MachineSessionMintResult, NewAuditEvent,
    NewCiAuditEvent, NewMachineSession, RepositoryError, ci_audit_event,
};
use blobyard_testkit::{ci_trust, github_oidc_identity};
use std::sync::Arc;

fn trust() -> LocalCiTrustRecord {
    ci_trust(
        "trust_fixture",
        "workspace_fixture",
        Some("project_fixture"),
        "http://127.0.0.1:8787",
        1,
    )
}

fn session() -> NewMachineSession {
    NewMachineSession {
        id: "machine_fixture".to_owned(),
        token_prefix: "byd_ci_fixture".to_owned(),
        secret_hash: "1".repeat(64),
        identity: github_oidc_identity("http://127.0.0.1:8787", "12345", 600_000),
        workspace: Some("fixture".to_owned()),
        project: "project".to_owned(),
        actions: vec![CiAction::Upload],
        oidc_token_hash: "2".repeat(64),
        now_ms: 10,
    }
}

fn event(action: &str, target_type: &str, target_id: &str, now_ms: u64) -> NewAuditEvent {
    ci_audit_event(NewCiAuditEvent {
        id: format!("event_{action}_{now_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "github:reliability-works/blobyard-core".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{now_ms}"),
        target_type: target_type.to_owned(),
        target_id: target_id.to_owned(),
        repository: blobyard_testkit::CI_REPOSITORY.to_owned(),
        created_at_ms: now_ms,
    })
}

#[test]
fn ci_fault_wrapper_forwards_every_operation() {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let trust = trust();
    FaultingRepository::new(Arc::clone(&inner), 1)
        .create_ci_trust(&trust, &event("ci.trust_created", "ci_trust", &trust.id, 1))
        .expect("forward trust creation");
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 1)
            .list_ci_trusts(&trust.workspace_id)
            .expect("forward trust listing"),
        vec![trust.clone()]
    );
    let session = session();
    assert!(matches!(
        FaultingRepository::new(Arc::clone(&inner), 1).mint_machine_session(
            &session,
            &event("ci.token_minted", "project", "project_fixture", 10),
        ),
        Ok(MachineSessionMintResult::Minted(_))
    ));
    assert!(
        FaultingRepository::new(Arc::clone(&inner), 1)
            .authenticate_machine_session(&session.id, 11)
            .is_ok()
    );
    assert_eq!(
        FaultingRepository::new(inner, 1).revoke_ci_trust(
            &trust.id,
            &trust.workspace_id,
            20,
            &event("ci.trust_revoked", "ci_trust", &trust.id, 20),
        ),
        Ok(true)
    );
}

#[test]
fn ci_fault_wrapper_fails_every_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let trust = trust();
    let session = session();
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0)
            .create_ci_trust(&trust, &event("ci.trust_created", "ci_trust", &trust.id, 1),),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).list_ci_trusts(&trust.workspace_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).revoke_ci_trust(
            &trust.id,
            &trust.workspace_id,
            20,
            &event("ci.trust_revoked", "ci_trust", &trust.id, 20),
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).mint_machine_session(
            &session,
            &event("ci.token_minted", "project", "project_fixture", 10),
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(inner, 0).authenticate_machine_session(&session.id, 11),
        Err(RepositoryError::Unavailable)
    );
}
