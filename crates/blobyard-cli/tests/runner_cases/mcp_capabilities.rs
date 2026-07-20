//! MCP discovery and capability management coverage.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure, ok};
use blobyard_core::ErrorCode;
use blobyard_mcp::{BackendError, Scope, ToolBackend, ToolCall};

fn scoped_fixture(responses: Vec<blobyard_api_client::RawResponse>) -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "mobile",
            "whoami",
        ],
        responses,
        Some("token"),
        None,
    )
}

#[tokio::test]
async fn lists_and_revokes_public_capabilities_by_stable_identifier() {
    let fixture = scoped_fixture(vec![
        share_page(),
        preview_page(),
        ok(serde_json::json!({}), "req_revoke"),
    ]);
    let scope = Scope::default();
    let shares = fixture
        .runner
        .call(ToolCall::ListShares {
            scope: scope.clone(),
        })
        .await
        .expect("list shares");
    let previews = fixture
        .runner
        .call(ToolCall::ListPreviews {
            scope: scope.clone(),
        })
        .await
        .expect("list previews");
    let revoked = fixture
        .runner
        .call(ToolCall::RevokePreview {
            scope,
            preview_id: "preview_1".to_owned(),
        })
        .await
        .expect("revoke preview");
    assert_eq!(shares["items"][0]["id"], "share_1");
    assert_eq!(previews["items"][0]["id"], "preview_1");
    assert_eq!(revoked, serde_json::json!({}));
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].query(), Some("workspace=team"));
    assert_eq!(requests[1].query(), Some("workspace=team&project=mobile"));
    assert_eq!(
        requests[2].body(),
        Some(&serde_json::json!({ "previewId": "preview_1" }))
    );
}

fn share_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "items": [{
                "id": "share_1", "expiresAt": "2026-07-18T00:00:00Z", "status": "active",
                "consumedCount": 0, "maximumDownloads": null
            }],
            "nextCursor": null
        }),
        "req_shares",
    )
}

fn preview_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "items": [{
                "id": "preview_1", "createdAt": "2026-07-11T00:00:00Z",
                "expiresAt": "2026-07-18T00:00:00Z", "status": "active"
            }],
            "nextCursor": null
        }),
        "req_previews",
    )
}

async fn assert_api_failure(call: ToolCall) {
    let fixture = scoped_fixture(vec![api_failure(ErrorCode::Forbidden, "req_forbidden")]);
    let error = fixture.runner.call(call).await.expect_err("forbidden call");
    assert_eq!(
        error,
        BackendError::new("FORBIDDEN", "You don't have access to do that.")
    );
}

#[tokio::test]
async fn discovery_and_revocation_propagate_safe_failures() {
    assert_api_failure(ToolCall::ListShares {
        scope: Scope::default(),
    })
    .await;
    assert_api_failure(ToolCall::ListPreviews {
        scope: Scope::default(),
    })
    .await;
    assert_api_failure(ToolCall::RevokePreview {
        scope: Scope::default(),
        preview_id: "preview_1".to_owned(),
    })
    .await;
}

#[tokio::test]
async fn discovery_requires_explicit_scope() {
    let fixture = Fixture::new(&["blobyard", "whoami"], Vec::new(), Some("token"), None);
    for (call, message) in missing_scope_cases() {
        let error = fixture.runner.call(call).await.expect_err("missing scope");
        assert_eq!(error, BackendError::new("INVALID_REQUEST", message));
    }
}

fn missing_scope_cases() -> [(ToolCall, &'static str); 2] {
    [
        (
            ToolCall::ListShares {
                scope: Scope::default(),
            },
            "Select a workspace with --workspace or configuration.",
        ),
        (
            ToolCall::ListPreviews {
                scope: Scope::default(),
            },
            "Select a workspace with --workspace or Blobyard configuration.",
        ),
    ]
}
