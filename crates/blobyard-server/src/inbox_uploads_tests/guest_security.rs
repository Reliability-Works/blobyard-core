use super::*;

async fn assert_guest_status_error(
    fixture: &test_seams::TransferFixture,
    path: &str,
    token: &str,
    bearer: bool,
    status: StatusCode,
    code: &str,
) {
    assert_error(
        guest_send(fixture, "GET", path, b"", Some(token), None, bearer).await,
        status,
        code,
    )
    .await;
}

async fn revoke(fixture: &test_seams::TransferFixture, inbox_id: &str) {
    let body =
        serde_json::to_vec(&serde_json::json!({ "inboxId": inbox_id })).expect("revoke request");
    assert_eq!(
        send(fixture, "POST", "/v1/inboxes/revoke", &body, false)
            .await
            .status(),
        StatusCode::OK
    );
}

async fn assert_cross_inbox_parts_are_concealed(
    fixture: &test_seams::TransferFixture,
    upload_id: &str,
    token: &str,
) {
    let parts = serde_json::to_vec(&serde_json::json!({
        "uploadId": upload_id,
        "partNumbers": [1]
    }))
    .expect("parts request");
    assert_error(
        guest_send(
            fixture,
            "POST",
            "/v1/uploads/parts/request",
            &parts,
            Some(token),
            None,
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn inbox_guest_authority_is_exclusive_and_immediately_revocable() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let (first_id, first) = create_inbox(&fixture, "First").await;
    let (_second_id, second) = create_inbox(&fixture, "Second").await;
    let body = upload_body("file.txt", 1, &hash("x"));
    let issued = issue(&fixture, &first, "owned", &body).await;
    let upload_id = issued["data"]["uploadId"].as_str().expect("upload ID");
    let status_path = format!("/v1/uploads/status?uploadId={upload_id}");
    assert_guest_status_error(
        &fixture,
        &status_path,
        &second,
        false,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_cross_inbox_parts_are_concealed(&fixture, upload_id, &second).await;
    assert_guest_status_error(
        &fixture,
        &status_path,
        &first,
        true,
        StatusCode::UNAUTHORIZED,
        "INVALID_TOKEN",
    )
    .await;
    assert_error(
        guest_send(
            &fixture,
            "POST",
            "/v1/uploads/request",
            b"{",
            Some("malformed"),
            Some("malformed"),
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    revoke(&fixture, &first_id).await;
    assert_guest_status_error(
        &fixture,
        &status_path,
        &first,
        false,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn inbox_guest_upload_requests_enforce_the_exact_hourly_limit() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let (_id, token) = create_inbox(&fixture, "Bounded").await;
    let body = upload_body("file.txt", 1, &hash("x"));
    for attempt in 0..20 {
        let response = guest_send(
            &fixture,
            "POST",
            "/v1/uploads/request",
            &body,
            Some(&token),
            Some(&format!("rate-{attempt}")),
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "attempt {attempt}");
    }
    assert_error(
        guest_send(
            &fixture,
            "POST",
            "/v1/uploads/request",
            &body,
            Some(&token),
            Some("rate-20"),
            false,
        )
        .await,
        StatusCode::TOO_MANY_REQUESTS,
        "RATE_LIMITED",
    )
    .await;
}
