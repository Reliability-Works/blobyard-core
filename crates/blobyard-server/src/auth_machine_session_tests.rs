use super::*;

fn ci_event(
    action: &str,
    target_type: &str,
    target_id: &str,
    workspace_id: &str,
    repository: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{target_id}"),
        workspace_id: workspace_id.to_owned(),
        actor: "github:reliability-works/blobyard-core".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{target_id}"),
        target_type: target_type.to_owned(),
        metadata: vec![
            (
                "repository".to_owned(),
                AuditValue::String(repository.to_owned()),
            ),
            (
                "targetId".to_owned(),
                AuditValue::String(target_id.to_owned()),
            ),
        ],
        created_at_ms,
    }
}

fn machine_session(
    fixture: &crate::transfers::test_seams::TransferFixture,
    raw_token: &str,
    repository: &str,
) -> NewMachineSession {
    NewMachineSession {
        id: "machine_auth_fixture".to_owned(),
        token_prefix: "byd_ci_auth".to_owned(),
        secret_hash: hash(raw_token),
        identity: GithubOidcIdentity {
            audience: fixture.state.public_origin.clone(),
            repository: repository.to_owned(),
            git_ref: "refs/heads/main".to_owned(),
            workflow_path: ".github/workflows/release.yml".to_owned(),
            workflow_ref: "refs/heads/main".to_owned(),
            environment: None,
            run_id: "auth-test".to_owned(),
            run_attempt: Some("1".to_owned()),
            sha: Some("a".repeat(40)),
            expires_at_ms: 1_000,
        },
        workspace: Some(fixture.state.default_workspace.slug.to_string()),
        project: fixture.project.slug.to_string(),
        actions: vec![CiAction::Upload],
        oidc_token_hash: hash("auth-assertion"),
        now_ms: 10,
    }
}

fn install_machine_session(
    fixture: &crate::transfers::test_seams::TransferFixture,
    raw_token: &str,
) {
    let repository = "reliability-works/blobyard-core";
    let trust = blobyard_testkit::ci_trust(
        "trust_auth_fixture",
        &fixture.principal.workspace_id,
        Some(&fixture.project.id),
        &fixture.state.public_origin,
        1,
    );
    let trust_event = ci_event(
        "ci.trust_created",
        "ci_trust",
        &trust.id,
        &trust.workspace_id,
        repository,
        1,
    );
    fixture
        .state
        .repository
        .create_ci_trust(&trust, &trust_event)
        .expect("create machine trust");
    let session = machine_session(fixture, raw_token, repository);
    let session_event = ci_event(
        "ci.token_minted",
        "project",
        &fixture.project.id,
        &fixture.principal.workspace_id,
        repository,
        10,
    );
    assert!(matches!(
        fixture
            .state
            .repository
            .mint_machine_session(&session, &session_event),
        Ok(MachineSessionMintResult::Minted(_))
    ));
}

#[test]
fn authentication_rejects_a_machine_token_corrupted_after_minting() {
    let fixture = crate::transfers::test_seams::fixture(&["object:read"]);
    install_machine_session(&fixture, "machine-secret");
    fixture
        .state
        .repository
        .create_project(&ProjectRecord {
            id: "project_other".to_owned(),
            workspace_id: fixture.principal.workspace_id.clone(),
            name: "Other".to_owned(),
            slug: Slug::new("other").expect("other project slug"),
        })
        .expect("other project");
    fixture.corrupt_machine_project("machine-secret");
    let error = super::super::test_seams::authenticate_at(&fixture.state, "machine-secret", 11)
        .expect_err("corrupted machine record");
    assert_eq!(error.into_response().status(), StatusCode::UNAUTHORIZED);
}
