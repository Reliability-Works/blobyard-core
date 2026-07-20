//! Inbox runner behavior over the typed API seam.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

#[tokio::test]
async fn creates_an_inbox_and_returns_its_capability_once() {
    let create = Fixture::new(
        &[
            "blobyard",
            "inbox",
            "create",
            "Client Logs",
            "--workspace",
            "team",
            "--project",
            "mobile",
            "--expires",
            "24h",
        ],
        vec![ok(
            serde_json::json!({
                "id": "inbox_1",
                "inboxUrl": "https://blobyard.com/i/raw-capability",
                "expiresAt": "2026-07-10T00:00:00Z"
            }),
            "req_inbox",
        )],
        Some("ci-token"),
        None,
    );
    let result = create
        .runner
        .execute(&create.command)
        .await
        .expect("create inbox");
    assert_eq!(result_json(result)["data"]["id"], "inbox_1");
}

#[tokio::test]
async fn lists_inboxes_without_capabilities() {
    let list = Fixture::new(
        &[
            "blobyard",
            "inbox",
            "list",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        vec![ok(
            serde_json::json!({
                "items": [{
                    "id": "inbox_1",
                    "name": "Client Logs",
                    "expiresAt": "2026-07-10T00:00:00Z",
                    "revoked": false
                }],
                "nextCursor": null
            }),
            "req_inboxes",
        )],
        Some("ci-token"),
        None,
    );
    let result = list
        .runner
        .execute(&list.command)
        .await
        .expect("list inboxes");
    let json = result_json(result);
    assert_eq!(json["data"]["items"][0]["name"], "Client Logs");
    assert!(json["data"]["items"][0].get("inboxUrl").is_none());
}

#[tokio::test]
async fn revokes_an_inbox_by_stable_identifier() {
    let revoke = Fixture::new(
        &["blobyard", "inbox", "revoke", "inbox_1"],
        vec![ok(serde_json::json!({}), "req_revoke")],
        Some("ci-token"),
        None,
    );
    revoke
        .runner
        .execute(&revoke.command)
        .await
        .expect("revoke inbox");
    assert_eq!(
        revoke.transport.requests()[0].endpoint(),
        Endpoint::RevokeInbox
    );
}

#[tokio::test]
async fn empty_inbox_listing_is_a_successful_local_presentation_path() {
    let empty = Fixture::new(
        &[
            "blobyard",
            "inbox",
            "list",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        vec![ok(
            serde_json::json!({ "items": [], "nextCursor": null }),
            "req_empty",
        )],
        Some("ci-token"),
        None,
    );
    empty
        .runner
        .execute(&empty.command)
        .await
        .expect("empty inboxes");
}

#[tokio::test]
async fn inbox_validation_rejects_bad_names_durations_and_identifiers() {
    let cases: &[&[&str]] = &[
        &[
            "blobyard",
            "inbox",
            "create",
            "",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        &[
            "blobyard",
            "inbox",
            "create",
            "line\nbreak",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        &[
            "blobyard",
            "inbox",
            "create",
            "name",
            "--workspace",
            "team",
            "--project",
            "app",
            "--expires",
            "never",
        ],
        &["blobyard", "inbox", "revoke", "bad id"],
        &["blobyard", "inbox", "revoke", ""],
    ];
    for args in cases {
        let fixture = Fixture::new(args, Vec::new(), Some("ci-token"), None);
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid inbox")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}

#[tokio::test]
async fn inbox_validation_rejects_overlong_names() {
    let long_name = "x".repeat(129);
    let fixture = Fixture::new(
        &[
            "blobyard",
            "inbox",
            "create",
            &long_name,
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert!(fixture.runner.execute(&fixture.command).await.is_err());
}
