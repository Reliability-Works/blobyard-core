use super::*;

fn assert_aborted_without_operator_audit(fixture: &test_seams::TransferFixture, upload_id: &str) {
    assert_eq!(
        fixture
            .state
            .repository
            .upload_by_id(upload_id)
            .expect("aborted upload")
            .state,
        ReservationState::Aborted
    );
    let audit = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 20)
        .expect("audit");
    assert!(
        audit
            .items
            .iter()
            .all(|event| !event.action.starts_with("transfer.upload_"))
    );
}

#[tokio::test]
async fn inbox_guest_can_issue_parts_and_abort_multipart_without_operator_audit() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let (_id, token) = create_inbox(&fixture, "Large files").await;
    let size = grants::SINGLE_UPLOAD_LIMIT_BYTES + 1;
    let body = upload_body("large.bin", size, &"0".repeat(64));
    let issued = issue(&fixture, &token, "multipart", &body).await;
    assert_eq!(issued["data"]["strategy"], "multipart");
    assert!(issued["data"]["uploadUrl"].is_null());
    let upload_id = issued["data"]["uploadId"].as_str().expect("upload ID");
    let parts = serde_json::to_vec(&serde_json::json!({
        "uploadId": upload_id,
        "partNumbers": [1, 2]
    }))
    .expect("parts request");
    let part_response = guest_send(
        &fixture,
        "POST",
        "/v1/uploads/parts/request",
        &parts,
        Some(&token),
        None,
        false,
    )
    .await;
    assert_eq!(part_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(part_response).await["data"]["parts"]
            .as_array()
            .expect("parts")
            .len(),
        2
    );
    let abort =
        serde_json::to_vec(&serde_json::json!({ "uploadId": upload_id })).expect("abort request");
    assert_eq!(
        guest_send(
            &fixture,
            "POST",
            "/v1/uploads/abort",
            &abort,
            Some(&token),
            None,
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_aborted_without_operator_audit(&fixture, upload_id);
}
