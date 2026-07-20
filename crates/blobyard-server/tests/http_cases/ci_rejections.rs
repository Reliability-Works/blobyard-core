use super::*;

#[tokio::test]
async fn ci_management_requires_its_exact_scope() {
    let fixture = test_seams::fixture_without_ci_management();
    for (method, uri, body) in [
        ("POST", "/v1/ci/trusts", Some(trust_request(&["upload"]))),
        ("GET", "/v1/ci/trusts?workspace=fixture", None),
        (
            "POST",
            "/v1/ci/trusts/revoke",
            Some(json!({ "trustId": "trust_missing" })),
        ),
    ] {
        let denied = support::send(
            &fixture.router(),
            method,
            uri,
            body,
            Some(fixture.operator_token()),
        )
        .await;
        assert_eq!(denied.0, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn ci_trust_creation_rejects_invalid_actions_and_json() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    for body in [
        trust_request(&[]),
        trust_request(&["upload", "upload"]),
        trust_request(&["unknown"]),
    ] {
        let invalid = support::send(
            &router,
            "POST",
            "/v1/ci/trusts",
            Some(body),
            Some(fixture.operator_token()),
        )
        .await;
        assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
    }
    let malformed = support::send_json_bytes(
        &router,
        "POST",
        "/v1/ci/trusts",
        b"{".to_vec(),
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ci_trust_management_conceals_missing_targets() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    for body in [
        json!({
            "allowedActions": ["upload"],
            "allowedRefGlob": "refs/heads/main",
            "project": "project",
            "repository": "reliability-works/blobyard-core",
            "workflowPath": ".github/workflows/release.yml",
            "workflowRef": "refs/heads/main",
            "workspace": "missing"
        }),
        json!({
            "allowedActions": ["upload"],
            "allowedRefGlob": "refs/heads/main",
            "project": "missing",
            "repository": "reliability-works/blobyard-core",
            "workflowPath": ".github/workflows/release.yml",
            "workflowRef": "refs/heads/main",
            "workspace": "fixture"
        }),
    ] {
        let missing = support::send(
            &router,
            "POST",
            "/v1/ci/trusts",
            Some(body),
            Some(fixture.operator_token()),
        )
        .await;
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
    }
    let missing = support::send(
        &router,
        "GET",
        "/v1/ci/trusts?workspace=missing",
        None,
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ci_trust_revoke_validates_and_is_idempotent() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    let malformed = support::send_json_bytes(
        &router,
        "POST",
        "/v1/ci/trusts/revoke",
        b"{".to_vec(),
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::BAD_REQUEST);
    for (trust_id, expected) in [
        ("", StatusCode::BAD_REQUEST),
        ("trust_missing", StatusCode::NOT_FOUND),
    ] {
        let missing = support::send(
            &router,
            "POST",
            "/v1/ci/trusts/revoke",
            Some(json!({ "trustId": trust_id })),
            Some(fixture.operator_token()),
        )
        .await;
        assert_eq!(missing.0, expected);
    }
    let created = create_trust(&router, fixture.operator_token(), &["upload"]).await;
    let trust_id = created["data"]["id"].as_str().expect("trust ID");
    for expected in ["revoked", "already_revoked"] {
        let revoked = support::send(
            &router,
            "POST",
            "/v1/ci/trusts/revoke",
            Some(json!({ "trustId": trust_id })),
            Some(fixture.operator_token()),
        )
        .await;
        assert_eq!(revoked.0, StatusCode::OK);
        assert_eq!(revoked.1["data"], expected);
    }
}

#[tokio::test]
async fn ci_exchange_rejects_malformed_assertions_actions_and_json() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    let malformed_assertion = support::send(
        &router,
        "POST",
        "/v1/ci/github/oidc/exchange",
        Some(json!({
            "actions": ["upload"],
            "project": "project",
            "workspace": "fixture"
        })),
        Some("opaque"),
    )
    .await;
    assert_eq!(malformed_assertion.0, StatusCode::UNAUTHORIZED);
    let malformed_json = support::send_json_bytes(
        &router,
        "POST",
        "/v1/ci/github/oidc/exchange",
        b"{".to_vec(),
        Some("valid.malformed.body"),
    )
    .await;
    assert_eq!(malformed_json.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        exchange(&router, "valid.actions.invalid", &["unknown"])
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn ci_exchange_fails_closed_for_provider_action_and_replay_errors() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    let active = create_trust(&router, fixture.operator_token(), &["upload"]).await;
    assert!(active["data"]["id"].is_string());
    for (assertion, expected) in [
        ("invalid.jwt.sig", StatusCode::UNAUTHORIZED),
        ("unavailable.jwt.sig", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        assert_eq!(exchange(&router, assertion, &["upload"]).await.0, expected);
    }
    assert_eq!(
        exchange(&router, "valid.forbidden.1", &["download"])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let first = exchange(&router, "valid.replay.1", &["upload"]).await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(
        exchange(&router, "valid.replay.1", &["upload"]).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn ci_exchange_defaults_workspace_and_conceals_missing_targets() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    create_trust(&router, fixture.operator_token(), &["upload"]).await;
    assert_eq!(
        exchange_target(
            &router,
            "valid.default.workspace",
            &["upload"],
            None,
            "project"
        )
        .await
        .0,
        StatusCode::OK
    );
    for (index, (workspace, project)) in
        [(Some("missing"), "project"), (Some("fixture"), "missing")]
            .into_iter()
            .enumerate()
    {
        assert_eq!(
            exchange_target(
                &router,
                &format!("valid.target.{index}"),
                &["upload"],
                workspace,
                project,
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        exchange(&router, "invalid-record.jwt.sig", &["upload"])
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}
