use super::*;

struct MachineJourney {
    created: Value,
    fixture: test_seams::CiFixture,
    machine: String,
    router: axum::Router,
}

async fn machine_journey(actions: &[&str]) -> MachineJourney {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    let created = create_trust(&router, fixture.operator_token(), actions).await;
    let exchanged = exchange(&router, "valid.lifecycle.1", actions).await;
    assert_eq!(exchanged.0, StatusCode::OK);
    assert_eq!(exchanged.1["data"]["scopes"], json!(actions));
    let machine = exchanged.1["data"]["accessToken"]
        .as_str()
        .expect("machine token")
        .to_owned();
    MachineJourney {
        created,
        fixture,
        machine,
        router,
    }
}

async fn upload_ci_object(router: &axum::Router, machine: &str) {
    let reserved = support::send_idempotent(
        router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "fixture",
            "project": "project",
            "path": "ci/artifact.txt",
            "filename": "artifact.txt",
            "sizeBytes": 5,
            "checksumSha256": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "contentType": "text/plain"
        })),
        Some(machine),
        Some("ci-http-lifecycle"),
    )
    .await;
    assert_eq!(reserved.0, StatusCode::OK);
    let uploaded = support::send_bytes(
        router,
        "PUT",
        &support::transfer_path(&reserved.1, "uploadUrl"),
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(uploaded.0, StatusCode::NO_CONTENT);
    let upload_id = reserved.1["data"]["uploadId"].as_str().expect("upload ID");
    let completed = support::send(
        router,
        "POST",
        "/v1/uploads/complete",
        Some(json!({ "uploadId": upload_id, "parts": [] })),
        Some(machine),
    )
    .await;
    assert_eq!(completed.0, StatusCode::OK);
}

#[tokio::test]
async fn ci_trust_management_contract_is_redacted_and_normalized() {
    let fixture = test_seams::fixture();
    let router = fixture.router();
    let created = create_trust(&router, fixture.operator_token(), &["upload", "download"]).await;
    assert_eq!(
        created["data"]["repository"],
        "reliability-works/blobyard-core"
    );
    assert!(created["data"].get("audience").is_none());

    let mut empty_environment = trust_request(&["upload"]);
    empty_environment["environment"] = Value::String(String::new());
    let without_environment = support::send(
        &router,
        "POST",
        "/v1/ci/trusts",
        Some(empty_environment),
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(without_environment.0, StatusCode::OK);
    assert!(without_environment.1["data"]["environment"].is_null());

    let mut workspace_wide = trust_request(&["download"]);
    workspace_wide["project"] = Value::Null;
    let workspace_wide = support::send(
        &router,
        "POST",
        "/v1/ci/trusts",
        Some(workspace_wide),
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(workspace_wide.0, StatusCode::OK);
    assert!(workspace_wide.1["data"]["projectId"].is_null());

    let listed = support::send(
        &router,
        "GET",
        "/v1/ci/trusts?workspace=fixture",
        None,
        Some(fixture.operator_token()),
    )
    .await;
    assert_eq!(listed.0, StatusCode::OK);
    assert_eq!(listed.1["data"].as_array().map(Vec::len), Some(3));
}

#[tokio::test]
async fn ci_machine_uploads_and_lists_its_project_objects() {
    let journey = machine_journey(&["upload", "download"]).await;
    let whoami = support::send(
        &journey.router,
        "GET",
        "/v1/cli/whoami",
        None,
        Some(&journey.machine),
    )
    .await;
    assert_eq!(whoami.0, StatusCode::OK);
    assert_eq!(whoami.1["data"]["principalType"], "ci");
    assert_eq!(whoami.1["data"]["scopes"], json!(["upload", "download"]));

    upload_ci_object(&journey.router, &journey.machine).await;
    let objects = support::send(
        &journey.router,
        "GET",
        "/v1/objects?workspace=fixture&project=project&versions=false",
        None,
        Some(&journey.machine),
    )
    .await;
    assert_eq!(objects.0, StatusCode::OK);
    assert_eq!(objects.1["data"]["items"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn project_bound_tokens_conceal_foreign_object_deletion() {
    let journey = machine_journey(&["upload"]).await;
    let project_token = support::send(
        &journey.router,
        "POST",
        "/v1/api-tokens",
        Some(json!({
            "expiresInDays": 7,
            "name": "Project cleanup",
            "project": "project",
            "scopes": ["object:write"],
            "workspace": "fixture"
        })),
        Some(journey.fixture.operator_token()),
    )
    .await;
    assert_eq!(project_token.0, StatusCode::OK);
    let project_token = project_token.1["data"]["rawToken"]
        .as_str()
        .expect("project token");
    let concealed = support::send(
        &journey.router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": "blobyard://fixture/other/blocked.txt" })),
        Some(project_token),
    )
    .await;
    assert_eq!(concealed.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ci_machine_is_concealed_from_foreign_project_requests() {
    let journey = machine_journey(&["upload", "download"]).await;
    let denied_list = support::send(
        &journey.router,
        "GET",
        "/v1/objects?workspace=fixture&project=other&versions=false",
        None,
        Some(&journey.machine),
    )
    .await;
    assert_eq!(denied_list.0, StatusCode::NOT_FOUND);
    let denied_upload = support::send_idempotent(
        &journey.router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "fixture",
            "project": "other",
            "path": "blocked.txt",
            "filename": "blocked.txt",
            "sizeBytes": 1,
            "checksumSha256": "00".repeat(32),
            "contentType": "text/plain"
        })),
        Some(&journey.machine),
        Some("wrong-project"),
    )
    .await;
    assert_eq!(denied_upload.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ci_machine_is_concealed_from_foreign_upload_lifecycle() {
    let journey = machine_journey(&["upload", "download"]).await;
    let other = support::send_idempotent(
        &journey.router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "fixture",
            "project": "other",
            "path": "operator.txt",
            "filename": "operator.txt",
            "sizeBytes": 1,
            "checksumSha256": "00".repeat(32),
            "contentType": "text/plain"
        })),
        Some(journey.fixture.operator_token()),
        Some("operator-other-project"),
    )
    .await;
    assert_eq!(other.0, StatusCode::OK);
    let upload_id = other.1["data"]["uploadId"].as_str().expect("upload ID");
    for (method, uri, body) in [
        (
            "GET",
            format!("/v1/uploads/status?uploadId={upload_id}"),
            None,
        ),
        (
            "POST",
            "/v1/uploads/complete".to_owned(),
            Some(json!({ "uploadId": upload_id, "parts": [] })),
        ),
        (
            "POST",
            "/v1/uploads/abort".to_owned(),
            Some(json!({ "uploadId": upload_id })),
        ),
    ] {
        let denied =
            support::send(&journey.router, method, &uri, body, Some(&journey.machine)).await;
        assert_eq!(denied.0, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn ci_machine_corruption_and_trust_revocation_fail_closed() {
    let journey = machine_journey(&["upload", "download"]).await;
    let corrupt = exchange(&journey.router, "valid.corrupt.1", &["upload"]).await;
    assert_eq!(corrupt.0, StatusCode::OK);
    let corrupt_token = corrupt.1["data"]["accessToken"]
        .as_str()
        .expect("corrupt machine token");
    let missing_action = support::send(
        &journey.router,
        "GET",
        "/v1/objects?workspace=fixture&project=project&versions=false",
        None,
        Some(corrupt_token),
    )
    .await;
    assert_eq!(missing_action.0, StatusCode::FORBIDDEN);
    journey.fixture.corrupt_machine_project(corrupt_token);
    let rejected = support::send(
        &journey.router,
        "GET",
        "/v1/cli/whoami",
        None,
        Some(corrupt_token),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::UNAUTHORIZED);

    let trust_id = journey.created["data"]["id"].as_str().expect("trust ID");
    let revoked = support::send(
        &journey.router,
        "POST",
        "/v1/ci/trusts/revoke",
        Some(json!({ "trustId": trust_id })),
        Some(journey.fixture.operator_token()),
    )
    .await;
    assert_eq!(revoked.0, StatusCode::OK);
    assert_eq!(revoked.1["data"], "revoked");
    let rejected = support::send(
        &journey.router,
        "GET",
        "/v1/cli/whoami",
        None,
        Some(&journey.machine),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::UNAUTHORIZED);
}
