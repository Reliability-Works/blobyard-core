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
    for (method, path, body) in super::edge_tests::route_shapes() {
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
    }
}
