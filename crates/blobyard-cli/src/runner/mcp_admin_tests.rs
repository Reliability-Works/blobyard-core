#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::runner::login::tests::support::Fixture;

fn scope() -> Scope {
    Scope::default()
}

fn runner_with_scope() -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "whoami",
        ],
        vec![],
    )
}

fn create_ci_trust(environment: Option<&str>, allowed_actions: &[&str]) -> AdminToolCall {
    AdminToolCall::CreateCiTrust {
        scope: scope(),
        repository: "acme/artifacts".to_owned(),
        workflow_path: ".github/workflows/upload-artifacts.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        allowed_ref_glob: "refs/heads/main".to_owned(),
        allowed_actions: allowed_actions
            .iter()
            .map(|action| (*action).to_owned())
            .collect(),
        environment: environment.map(ToOwned::to_owned),
    }
}

fn read_calls() -> Vec<(AdminToolCall, Endpoint)> {
    vec![
        (
            AdminToolCall::ListAudit {
                scope: scope(),
                cursor: Some("next".to_owned()),
            },
            Endpoint::ListAudit,
        ),
        (
            AdminToolCall::ListMembers { scope: scope() },
            Endpoint::ListMembers,
        ),
        (
            AdminToolCall::ListInvites { scope: scope() },
            Endpoint::ListInvites,
        ),
        (
            AdminToolCall::ListApiTokens { scope: scope() },
            Endpoint::ListApiTokens,
        ),
        (
            AdminToolCall::ListCiTrusts { scope: scope() },
            Endpoint::ListCiTrusts,
        ),
        (
            AdminToolCall::ListCliSessions { scope: scope() },
            Endpoint::ListCliSessions,
        ),
    ]
}

fn member_write_calls() -> Vec<(AdminToolCall, Endpoint)> {
    vec![
        (
            AdminToolCall::CreateInvite {
                scope: scope(),
                email: "developer@example.com".to_owned(),
                role: "member".to_owned(),
            },
            Endpoint::CreateInvite,
        ),
        (
            AdminToolCall::RevokeInvite {
                scope: scope(),
                invite_id: "invite_1".to_owned(),
                confirmed: true,
            },
            Endpoint::RevokeInvite,
        ),
        (
            AdminToolCall::UpdateMemberRole {
                scope: scope(),
                user_id: "user_1".to_owned(),
                role: "admin".to_owned(),
                confirmed: true,
            },
            Endpoint::UpdateMemberRole,
        ),
        (
            AdminToolCall::RemoveMember {
                scope: scope(),
                user_id: "user_1".to_owned(),
                confirmed: true,
            },
            Endpoint::RemoveMember,
        ),
    ]
}

fn credential_write_calls() -> Vec<(AdminToolCall, Endpoint)> {
    vec![
        (
            AdminToolCall::RevokeApiToken {
                scope: scope(),
                token_id: "token_1".to_owned(),
                confirmed: true,
            },
            Endpoint::RevokeApiToken,
        ),
        (
            create_ci_trust(Some("Production"), &["upload", "share"]),
            Endpoint::CreateCiTrust,
        ),
        (
            AdminToolCall::RevokeCiTrust {
                scope: scope(),
                trust_id: "trust_1".to_owned(),
                confirmed: true,
            },
            Endpoint::RevokeCiTrust,
        ),
        (
            AdminToolCall::RevokeCliSession {
                scope: scope(),
                session_id: "session_1".to_owned(),
                confirmed: true,
            },
            Endpoint::RevokeCliSession,
        ),
    ]
}

#[test]
fn maps_every_administration_call_to_its_versioned_endpoint() {
    let fixture = runner_with_scope();
    for (call, endpoint) in read_calls()
        .into_iter()
        .chain(member_write_calls())
        .chain(credential_write_calls())
    {
        assert_eq!(admin_scope(&call), &scope());
        let request = admin_request(&fixture.runner, call).expect("administration request");
        assert_eq!(request.endpoint(), endpoint);
    }
}

#[test]
fn builds_exact_workspace_queries_and_mutation_bodies() {
    let fixture = runner_with_scope();
    let audit = admin_request(
        &fixture.runner,
        AdminToolCall::ListAudit {
            scope: scope(),
            cursor: None,
        },
    )
    .expect("audit request");
    assert_eq!(audit.query(), Some("workspace=main"));

    let invite = admin_request(
        &fixture.runner,
        AdminToolCall::CreateInvite {
            scope: scope(),
            email: "developer@example.com".to_owned(),
            role: "member".to_owned(),
        },
    )
    .expect("invite request");
    assert_eq!(
        invite.body().and_then(|body| body["workspace"].as_str()),
        Some("main")
    );

    let trust = admin_request(
        &fixture.runner,
        create_ci_trust(Some("Production"), &["upload"]),
    )
    .expect("trust request");
    assert_eq!(
        trust.body().and_then(|body| body["project"].as_str()),
        Some("artifacts")
    );
    assert_eq!(
        trust.body().and_then(|body| body["environment"].as_str()),
        Some("Production")
    );
}

#[test]
fn omits_unselected_project_and_environment() {
    let fixture = Fixture::new(&["blobyard", "--workspace", "main", "whoami"], vec![]);
    let request =
        admin_request(&fixture.runner, create_ci_trust(None, &["upload"])).expect("trust request");
    let body = request.body().expect("request body");
    assert!(body.get("project").is_none());
    assert!(body.get("environment").is_none());
}

#[test]
fn rejects_missing_workspace_and_non_object_mutation_bodies() {
    let fixture = Fixture::new(&["blobyard", "whoami"], vec![]);
    assert!(
        admin_request(
            &fixture.runner,
            AdminToolCall::ListMembers { scope: scope() }
        )
        .is_err()
    );
    assert!(
        admin_request(
            &fixture.runner,
            AdminToolCall::ListAudit {
                scope: scope(),
                cursor: Some("next".to_owned()),
            },
        )
        .is_err()
    );
    assert!(
        workspace_write(
            &fixture.runner,
            Endpoint::CreateInvite,
            &json!({ "email": "developer@example.com" }),
        )
        .is_err()
    );
    assert!(workspace_write(&fixture.runner, Endpoint::CreateInvite, &Value::Null).is_err());
}

#[test]
fn destructive_administration_requires_explicit_confirmation() {
    let destructive = [
        AdminToolCall::RevokeInvite {
            scope: scope(),
            invite_id: "invite_1".to_owned(),
            confirmed: false,
        },
        AdminToolCall::UpdateMemberRole {
            scope: scope(),
            user_id: "user_1".to_owned(),
            role: "member".to_owned(),
            confirmed: false,
        },
        AdminToolCall::RemoveMember {
            scope: scope(),
            user_id: "user_1".to_owned(),
            confirmed: false,
        },
        AdminToolCall::RevokeApiToken {
            scope: scope(),
            token_id: "token_1".to_owned(),
            confirmed: false,
        },
        AdminToolCall::RevokeCiTrust {
            scope: scope(),
            trust_id: "trust_1".to_owned(),
            confirmed: false,
        },
        AdminToolCall::RevokeCliSession {
            scope: scope(),
            session_id: "session_1".to_owned(),
            confirmed: false,
        },
    ];
    for call in destructive {
        let error = require_admin_confirmation(&call).expect_err("confirmation must be required");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }
    assert!(require_admin_confirmation(&AdminToolCall::ListMembers { scope: scope() }).is_ok());
}

#[tokio::test]
async fn administration_execution_rejects_unconfirmed_mutations_before_transport() {
    let fixture = runner_with_scope();
    let call = AdminToolCall::RevokeInvite {
        scope: scope(),
        invite_id: "invite_1".to_owned(),
        confirmed: false,
    };
    let error = fixture
        .runner
        .execute_mcp_admin(call)
        .await
        .expect_err("unconfirmed mutation");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
    assert!(fixture.transport.requests().is_empty());
}
