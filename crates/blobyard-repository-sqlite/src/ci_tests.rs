#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::ci_validation;
use blobyard_contract::{
    CiAction, CiRepository, CredentialRepository, LocalCiTrustRecord, LocalMachineSessionRecord,
    MachineSessionMintResult, NewMachineSession, RepositoryError,
};

use super::super::ci_test_fixtures::{
    event, repository, repository_with_trust, repository_with_trust_and_session, session, trust,
};

fn minted_record(result: MachineSessionMintResult) -> Box<LocalMachineSessionRecord> {
    match result {
        MachineSessionMintResult::Minted(record) => Some(record),
        _ => None,
    }
    .expect("expected minted session")
}

#[test]
fn trust_exchange_authentication_and_revocation_are_atomic() {
    let (_temporary, repository, trust) = repository_with_trust();
    assert_eq!(
        repository.list_ci_trusts("workspace_fixture"),
        Ok(vec![trust.clone()])
    );

    let session = session(1, 10);
    let result = repository
        .mint_machine_session(
            &session,
            &event("ci.token_minted", "project", "project_fixture", 10),
        )
        .expect("mint session");
    let record = minted_record(result);
    assert_eq!(record.trust_id, trust.id);
    assert_eq!(record.actions, vec![CiAction::Upload]);
    assert!(repository.list_api_tokens().expect("api tokens").is_empty());

    let token = repository
        .authenticate_api_token(&session.secret_hash, 11)
        .expect("authenticate machine token");
    assert_eq!(token.id, session.id);
    assert_eq!(token.scopes, vec!["upload"]);
    assert_eq!(
        repository
            .authenticate_machine_session(&session.id, 12)
            .expect("authenticate session")
            .last_used_at_ms,
        Some(12)
    );

    assert!(
        repository
            .revoke_ci_trust(
                &trust.id,
                &trust.workspace_id,
                20,
                &event("ci.trust_revoked", "ci_trust", &trust.id, 20),
            )
            .expect("revoke trust")
    );
    assert!(
        !repository
            .revoke_ci_trust(
                &trust.id,
                &trust.workspace_id,
                21,
                &event("ci.trust_revoked", "ci_trust", &trust.id, 21),
            )
            .expect("idempotent revoke")
    );
    assert_eq!(
        repository.authenticate_api_token(&session.secret_hash, 22),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.authenticate_machine_session(&session.id, 22),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn exchange_fails_closed_for_replay_claim_mismatch_and_expiry() {
    let (_temporary, repository, _trust) = repository_with_trust();
    let original = session(1, 10);
    let minted = repository
        .mint_machine_session(
            &original,
            &event("ci.token_minted", "project", "project_fixture", 10),
        )
        .expect("mint original");
    assert!(matches!(minted, MachineSessionMintResult::Minted(_)));
    assert_eq!(
        repository.mint_machine_session(
            &original,
            &event("ci.token_minted", "project", "project_fixture", 10),
        ),
        Ok(MachineSessionMintResult::Replayed)
    );

    let mut mismatches = Vec::new();
    let mut audience = session(2, 20);
    audience.identity.audience = "https://other.example".to_owned();
    mismatches.push(audience);
    let mut workflow = session(3, 20);
    workflow.identity.workflow_path = ".github/workflows/other.yml".to_owned();
    mismatches.push(workflow);
    let mut workflow_ref = session(4, 20);
    workflow_ref.identity.workflow_ref = "refs/heads/other".to_owned();
    mismatches.push(workflow_ref);
    let mut git_ref = session(5, 20);
    git_ref.identity.git_ref = "refs/tags/v1".to_owned();
    mismatches.push(git_ref);
    let mut environment = session(6, 20);
    environment.identity.environment = Some("production".to_owned());
    mismatches.push(environment);
    let mut action = session(7, 20);
    action.actions = vec![CiAction::Download];
    mismatches.push(action);
    let mut workspace = session(8, 20);
    workspace.workspace = Some("other".to_owned());
    mismatches.push(workspace);
    let mut project = session(9, 20);
    project.project = "other".to_owned();
    mismatches.push(project);

    for mismatch in mismatches {
        assert_eq!(
            repository.mint_machine_session(
                &mismatch,
                &event("ci.token_minted", "project", "project_fixture", 20),
            ),
            Ok(MachineSessionMintResult::Forbidden)
        );
    }
    assert_eq!(
        repository.authenticate_api_token(&original.secret_hash, original.identity.expires_at_ms),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn project_specific_trust_wins_and_rate_limit_is_durable() {
    let (_temporary, repository) = repository();
    let general = trust("trust_general", None, 1);
    let specific = trust("trust_specific", Some("project_fixture"), 2);
    for trust in [&general, &specific] {
        repository
            .create_ci_trust(
                trust,
                &event(
                    "ci.trust_created",
                    "ci_trust",
                    &trust.id,
                    trust.created_at_ms,
                ),
            )
            .expect("create trust");
    }
    for index in 1..=20 {
        let now_ms = 100 + index;
        let session = session(index, now_ms);
        let result = repository
            .mint_machine_session(
                &session,
                &event("ci.token_minted", "project", "project_fixture", now_ms),
            )
            .expect("mint session");
        let record = minted_record(result);
        assert_eq!(record.trust_id, specific.id);
    }
    assert_eq!(
        repository.mint_machine_session(
            &session(21, 121),
            &event("ci.token_minted", "project", "project_fixture", 121),
        ),
        Ok(MachineSessionMintResult::RateLimited {
            retry_after_seconds: 60,
        })
    );
}

#[path = "ci_failure_tests.rs"]
mod failures;

#[path = "ci_boundary_tests.rs"]
mod boundaries;
