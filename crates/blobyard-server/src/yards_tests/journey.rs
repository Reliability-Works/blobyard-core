use super::{body, host, mutate, public_request, start, upload_manifest};
use crate::{contract_test_support::response_json, transfers::test_seams};
use axum::http::{StatusCode, header};

async fn assert_public_resolution(
    fixture: &test_seams::TransferFixture,
    host: &str,
    expected_index: &[u8],
) {
    for (path, status, expected) in [
        ("/", StatusCode::OK, expected_index),
        ("/asset.js", StatusCode::OK, b"yard asset".as_slice()),
        ("/docs/", StatusCode::OK, b"docs index".as_slice()),
        ("/guide", StatusCode::OK, b"clean guide".as_slice()),
        ("/client-route", StatusCode::OK, expected_index),
        (
            "/missing.txt",
            StatusCode::NOT_FOUND,
            b"not found".as_slice(),
        ),
    ] {
        assert_public_path(fixture, host, path, status, expected).await;
    }
    assert_head_and_range(fixture, host).await;
}

async fn assert_public_path(
    fixture: &test_seams::TransferFixture,
    host: &str,
    path: &str,
    status: StatusCode,
    expected: &[u8],
) {
    let response = public_request(fixture, "GET", path, host, None).await;
    assert_eq!(response.status(), status, "{path}");
    assert_eq!(body(response).await.as_ref(), expected, "{path}");
}

async fn assert_head_and_range(fixture: &test_seams::TransferFixture, host: &str) {
    let head = public_request(fixture, "HEAD", "/asset.js", host, None).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert!(body(head).await.is_empty());
    let range = public_request(fixture, "GET", "/asset.js", host, Some("bytes=0-3")).await;
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(range.headers()[header::CONTENT_RANGE], "bytes 0-3/10");
    assert_eq!(body(range).await.as_ref(), b"yard");
}

async fn publish(
    fixture: &test_seams::TransferFixture,
    client_deploy_id: &str,
    index: &[u8],
) -> serde_json::Value {
    let started = start(fixture, client_deploy_id).await;
    let root = started["data"]["manifestRoot"]
        .as_str()
        .expect("manifest root");
    upload_manifest(fixture, root, index).await;
    let live = mutate(
        fixture,
        "/v1/yards/deploys/finalise",
        serde_json::json!({ "deployId": started["data"]["deployId"] }),
    )
    .await;
    assert_eq!(live["data"]["status"], "live");
    started
}

async fn assert_current_yard(fixture: &test_seams::TransferFixture, first: &serde_json::Value) {
    let yards = response_json(
        crate::contract_test_support::send(
            fixture,
            "GET",
            "/v1/yards?workspace=fixture&project=project",
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(yards["data"]["items"][0]["id"], first["data"]["yardId"]);
    assert_eq!(
        yards["data"]["items"][0]["currentDeployId"],
        first["data"]["deployId"]
    );
}

async fn replace_and_assert_history(
    fixture: &test_seams::TransferFixture,
    first: &serde_json::Value,
    stable_host: &str,
    first_host: &str,
) {
    publish(fixture, "deploy-second-002", b"second index").await;
    assert_public_resolution(fixture, stable_host, b"second index").await;
    assert_public_resolution(fixture, first_host, b"first index").await;
    let history = response_json(
        crate::contract_test_support::send(
            fixture,
            "GET",
            &format!(
                "/v1/yards/deploys?yardId={}",
                first["data"]["yardId"].as_str().expect("yard ID")
            ),
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(history["data"]["items"][0]["status"], "live");
    assert_eq!(history["data"]["items"][1]["status"], "superseded");
}

async fn rollback_first(
    fixture: &test_seams::TransferFixture,
    first: &serde_json::Value,
    stable_host: &str,
) {
    let rolled_back = mutate(
        fixture,
        "/v1/yards/rollback",
        serde_json::json!({
            "yardId": first["data"]["yardId"],
            "deployId": first["data"]["deployId"]
        }),
    )
    .await;
    assert_eq!(rolled_back["data"]["deployId"], first["data"]["deployId"]);
    assert_public_resolution(fixture, stable_host, b"first index").await;
}

async fn fail_and_assert_audit(fixture: &test_seams::TransferFixture) {
    let failed = start(fixture, "deploy-failed-003").await;
    mutate(
        fixture,
        "/v1/yards/deploys/fail",
        serde_json::json!({
            "deployId": failed["data"]["deployId"],
            "failureCode": "UPLOAD_FAILED",
            "failureMessage": "The upload failed safely."
        }),
    )
    .await;
    let audit = response_json(
        crate::contract_test_support::send(
            fixture,
            "GET",
            "/v1/audit?workspace=fixture",
            b"",
            false,
        )
        .await,
    )
    .await;
    let actions = audit["data"]["items"]
        .as_array()
        .expect("audit items")
        .iter()
        .filter_map(|event| event["action"].as_str())
        .collect::<Vec<_>>();
    for expected in ["yard.created", "yard.deployed", "yard.rolled_back"] {
        assert!(actions.contains(&expected), "missing {expected}");
    }
}

async fn delete_and_assert_gone(
    fixture: &test_seams::TransferFixture,
    first: &serde_json::Value,
    stable_host: &str,
    first_host: &str,
) {
    for _attempt in 0..2 {
        mutate(
            fixture,
            "/v1/yards/delete",
            serde_json::json!({ "yardId": first["data"]["yardId"] }),
        )
        .await;
    }
    assert_eq!(
        public_request(fixture, "GET", "/", stable_host, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public_request(fixture, "GET", "/", first_host, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn web_yard_journey_preserves_immutable_deploys_and_controls_the_stable_alias() {
    let fixture = test_seams::fixture(&["audit:read", "object:write", "yard:manage", "yard:read"]);
    let first = publish(&fixture, "deploy-first-0001", b"first index").await;
    let stable_host = host(&first, "url");
    let first_host = host(&first, "deploymentUrl");
    assert_public_resolution(&fixture, &stable_host, b"first index").await;
    assert_public_resolution(&fixture, &first_host, b"first index").await;
    assert_current_yard(&fixture, &first).await;
    replace_and_assert_history(&fixture, &first, &stable_host, &first_host).await;
    rollback_first(&fixture, &first, &stable_host).await;
    fail_and_assert_audit(&fixture).await;
    delete_and_assert_gone(&fixture, &first, &stable_host, &first_host).await;
}
