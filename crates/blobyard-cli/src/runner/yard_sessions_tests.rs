#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{session_lines, status_label};
use crate::TokenStore;
use crate::runner::login::tests::support::{Fixture, ok};
use crate::yard_commands::{RevokeYardSessionArgs, YardSessionsListArgs};
use blobyard_api_client::{RawResponse, YardSessionStatus, YardSessionSummary};
use blobyard_core::{ErrorCode, SecretString};

fn session(status: YardSessionStatus, last_used_at: Option<&str>) -> YardSessionSummary {
    YardSessionSummary {
        created_at: "2026-07-24T09:00:00Z".to_owned(),
        environment_id: "yardenv_docs".to_owned(),
        expires_at: "2026-08-23T09:00:00Z".to_owned(),
        host_label: "docs-123456789-team".to_owned(),
        id: "byys_session".to_owned(),
        last_used_at: last_used_at.map(str::to_owned),
        status,
        user_display_name: "Avery Reader".to_owned(),
        user_id: "user_reader".to_owned(),
        yard_id: "yard_docs".to_owned(),
    }
}

#[test]
fn session_lines_cover_empty_and_populated_lists() {
    assert_eq!(session_lines(&[]), "No Yard browser sessions found.");
    assert_eq!(
        session_lines(&[
            session(YardSessionStatus::Active, Some("2026-07-24T10:00:00Z")),
            session(YardSessionStatus::Expired, None),
        ]),
        "byys_session\tactive\tAvery Reader\tdocs-123456789-team\tcreated \
         2026-07-24T09:00:00Z\texpires 2026-08-23T09:00:00Z\tlast used \
         2026-07-24T10:00:00Z\nbyys_session\texpired\tAvery Reader\t\
         docs-123456789-team\tcreated 2026-07-24T09:00:00Z\texpires \
         2026-08-23T09:00:00Z\tlast used never"
    );
}

#[test]
fn status_labels_cover_the_public_lifecycle() {
    assert_eq!(status_label(YardSessionStatus::Active), "active");
    assert_eq!(status_label(YardSessionStatus::Expired), "expired");
    assert_eq!(status_label(YardSessionStatus::Revoked), "revoked");
}

fn runner(responses: Vec<RawResponse>) -> Fixture {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--api-url",
            "http://127.0.0.1:8787",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "whoami",
        ],
        responses,
    );
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    fixture
}

fn yards() -> RawResponse {
    ok(
        &serde_json::json!({
            "items": [{
                "currentDeployId": "deploy_documentation",
                "hostLabel": "documentation-x",
                "id": "yard_documentation",
                "name": "documentation",
                "projectId": "project_artifacts",
                "status": "active",
                "url": "https://documentation-x.blobyard.app",
                "workspaceId": "workspace_main"
            }],
            "nextCursor": null
        }),
        "request_yards",
    )
}

fn tokens() -> RawResponse {
    ok(
        &serde_json::json!({
            "accessToken": "access-token-fixture",
            "refreshToken": "refresh-token-fixture",
            "expiresInSeconds": 900
        }),
        "request_tokens",
    )
}

#[tokio::test]
async fn session_commands_propagate_selection_transport_and_validation_failures() {
    assert_list_failures().await;
    assert_revoke_failures().await;
}

async fn assert_list_failures() {
    assert_eq!(
        runner(Vec::new())
            .runner
            .list_yard_sessions(&YardSessionsListArgs {
                name: Some("documentation".to_owned()),
            })
            .await
            .expect_err("yard transport failure")
            .code(),
        ErrorCode::InternalError
    );
    assert_eq!(
        runner(vec![tokens(), yards()])
            .runner
            .list_yard_sessions(&YardSessionsListArgs {
                name: Some("missing".to_owned()),
            })
            .await
            .expect_err("missing yard")
            .code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        runner(vec![tokens(), yards(), tokens()])
            .runner
            .list_yard_sessions(&YardSessionsListArgs {
                name: Some("documentation".to_owned()),
            })
            .await
            .expect_err("session transport failure")
            .code(),
        ErrorCode::InternalError
    );
}

async fn assert_revoke_failures() {
    for arguments in [
        RevokeYardSessionArgs {
            name: "api".to_owned(),
            session_id: "session_valid".to_owned(),
        },
        RevokeYardSessionArgs {
            name: "documentation".to_owned(),
            session_id: String::new(),
        },
    ] {
        assert_eq!(
            runner(Vec::new())
                .runner
                .revoke_yard_session(&arguments)
                .await
                .expect_err("invalid revoke input")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
    assert_eq!(
        runner(Vec::new())
            .runner
            .revoke_yard_session(&RevokeYardSessionArgs {
                name: "documentation".to_owned(),
                session_id: "session_valid".to_owned(),
            })
            .await
            .expect_err("yard transport failure")
            .code(),
        ErrorCode::InternalError
    );
    assert_eq!(
        runner(vec![tokens(), yards()])
            .runner
            .revoke_yard_session(&RevokeYardSessionArgs {
                name: "missing".to_owned(),
                session_id: "session_valid".to_owned(),
            })
            .await
            .expect_err("missing yard")
            .code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        runner(vec![tokens(), yards(), tokens()])
            .runner
            .revoke_yard_session(&RevokeYardSessionArgs {
                name: "documentation".to_owned(),
                session_id: "session_valid".to_owned(),
            })
            .await
            .expect_err("session transport failure")
            .code(),
        ErrorCode::InternalError
    );
}
