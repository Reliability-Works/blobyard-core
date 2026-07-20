use crate::support;
use axum::http::StatusCode;
use blobyard_server::api_ci_exchange::test_seams;
use serde_json::{Value, json};

fn trust_request(actions: &[&str]) -> Value {
    json!({
        "allowedActions": actions,
        "allowedRefGlob": "refs/heads/main",
        "project": "project",
        "repository": "Reliability-Works/Blobyard-Core",
        "workflowPath": ".github/workflows/release.yml",
        "workflowRef": "refs/heads/main",
        "workspace": "fixture"
    })
}

async fn create_trust(router: &axum::Router, token: &str, actions: &[&str]) -> Value {
    let created = support::send(
        router,
        "POST",
        "/v1/ci/trusts",
        Some(trust_request(actions)),
        Some(token),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK);
    created.1
}

async fn exchange(router: &axum::Router, assertion: &str, actions: &[&str]) -> (StatusCode, Value) {
    exchange_target(router, assertion, actions, Some("fixture"), "project").await
}

async fn exchange_target(
    router: &axum::Router,
    assertion: &str,
    actions: &[&str],
    workspace: Option<&str>,
    project: &str,
) -> (StatusCode, Value) {
    let mut body = json!({
        "actions": actions,
        "project": project
    });
    if let Some(workspace) = workspace {
        body["workspace"] = Value::String(workspace.to_owned());
    }
    support::send(
        router,
        "POST",
        "/v1/ci/github/oidc/exchange",
        Some(body),
        Some(assertion),
    )
    .await
}

#[tokio::test]
async fn ci_http_enforces_the_durable_exchange_rate_limit() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    create_trust(&router, fixture.operator_token(), &["upload"]).await;
    for index in 0..20 {
        let response = exchange(&router, &format!("valid.rate.{index}"), &["upload"]).await;
        assert_eq!(response.0, StatusCode::OK);
    }
    let limited = exchange(&router, "valid.rate.limited", &["upload"]).await;
    assert_eq!(limited.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.1["error"]["code"], "RATE_LIMITED");
}

#[tokio::test]
async fn ci_http_maps_trust_repository_failures_without_storage_details() {
    let broken_trusts = test_seams::fixture();
    broken_trusts.break_ci_trusts();
    for (method, uri, body) in [
        ("GET", "/v1/ci/trusts?workspace=fixture", None),
        ("POST", "/v1/ci/trusts", Some(trust_request(&["upload"]))),
        (
            "POST",
            "/v1/ci/trusts/revoke",
            Some(json!({ "trustId": "trust_missing" })),
        ),
    ] {
        let failed = support::send(
            &broken_trusts.router(),
            method,
            uri,
            body,
            Some(broken_trusts.operator_token()),
        )
        .await;
        support::assert_internal(failed.0, &failed.1);
    }
}

#[tokio::test]
async fn ci_http_maps_secondary_repository_and_clock_failures() {
    let broken_workspaces = test_seams::fixture();
    broken_workspaces.break_workspace_listing();
    let failed = support::send(
        &broken_workspaces.router(),
        "POST",
        "/v1/ci/trusts/revoke",
        Some(json!({ "trustId": "trust_missing" })),
        Some(broken_workspaces.operator_token()),
    )
    .await;
    support::assert_internal(failed.0, &failed.1);

    let broken_revoke = test_seams::fixture();
    let created = create_trust(
        &broken_revoke.router(),
        broken_revoke.operator_token(),
        &["upload"],
    )
    .await;
    let trust_id = created["data"]["id"].as_str().expect("trust ID");
    broken_revoke.break_ci_revoke();
    let failed = support::send(
        &broken_revoke.router(),
        "POST",
        "/v1/ci/trusts/revoke",
        Some(json!({ "trustId": trust_id })),
        Some(broken_revoke.operator_token()),
    )
    .await;
    support::assert_internal(failed.0, &failed.1);

    assert_eq!(
        test_seams::clock_failure_status().await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        blobyard_server::api_ci_trusts::test_seams::failure_statuses(),
        [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR
        ]
    );
}

#[path = "ci_lifecycle.rs"]
mod lifecycle;

#[path = "ci_rejections.rs"]
mod rejections;
