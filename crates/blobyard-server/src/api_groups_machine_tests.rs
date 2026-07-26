#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, send_as};
use crate::transfers::test_seams;
use axum::http::StatusCode;

#[tokio::test]
async fn every_group_route_rejects_machine_principals() {
    let fixture = test_seams::fixture(&["object:read"]);
    crate::test_support::install_machine_session(
        &fixture,
        "machine-secret",
        "group_fixture",
        crate::transfer_grants::now_ms().expect("current time"),
    );
    for (fixture_id, operation_id, method, path, body) in super::edge_tests::route_shapes() {
        assert_error(
            send_as(
                test_seams::fixture_router(&fixture.state),
                "machine-secret",
                method,
                path,
                body,
            )
            .await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
        blobyard_testkit::assert_group_authorization_fixture_case(
            fixture_id,
            &serde_json::json!({
                "action": "users:manage",
                "expected": {"allowed": false, "code": "FORBIDDEN"},
                "id": fixture_id,
                "operationId": operation_id,
                "principalKind": "machine",
                "principalScopes": ["users:manage"],
                "resource": {"project": "demo", "workspace": "default"}
            }),
        )
        .expect("machine authorization vector");
    }
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("server", "group-machine");
    tracker.record_case(
        "machine-principal-is-denied-each-group-operation",
        &serde_json::json!({
            "principalKind": "machine",
            "operations": [
                "list-groups",
                "create-group",
                "rename-group",
                "list-group-members",
                "add-group-member",
                "remove-group-member",
                "deactivate-group"
            ]
        }),
        &serde_json::json!({
            "allowed": false,
            "code": "FORBIDDEN",
            "deniedOperationCount": 7
        }),
    );
    tracker.finish().expect("complete machine group fixtures");
}
