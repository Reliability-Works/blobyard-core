use super::*;

async fn assert_invalid_requests(state: &AppState) {
    for numbers in [
        serde_json::json!([]),
        serde_json::json!([1, 1]),
        serde_json::json!([0]),
        serde_json::json!([3]),
    ] {
        let response = send_json(
            state,
            "POST",
            "/v1/uploads/parts/request",
            serde_json::json!({"uploadId": "upload_multipart", "partNumbers": numbers}),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json(response).await["error"]["code"], "INVALID_REQUEST");
    }
}

fn assert_redacted(root: &tempfile::TempDir, raw_capability: &str) {
    let connection = rusqlite::Connection::open(root.path().join("metadata.sqlite3"))
        .expect("metadata connection");
    let capability_hash: String = connection
        .query_row(
            "SELECT capability_hash FROM upload_parts WHERE upload_id = 'upload_multipart' AND part_number = 1",
            [],
            |row| row.get(0),
        )
        .expect("capability hash");
    assert_ne!(capability_hash, raw_capability);
    let audit_json: String = connection
        .query_row(
            "SELECT metadata_json FROM audit_events WHERE action = 'transfer.upload_parts_requested' ORDER BY created_at_ms DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("audit metadata");
    assert!(!audit_json.contains(raw_capability));
}

async fn assert_abort(state: &AppState, reservation: UploadReservationRecord) {
    let response = send_json(
        state,
        "POST",
        "/v1/uploads/abort",
        serde_json::json!({"uploadId": reservation.id}),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        state
            .repository
            .list_upload_parts("upload_multipart")
            .expect("parts")
            .is_empty()
    );
    assert_eq!(
        state
            .storage
            .abort_multipart(&blobyard_contract::MultipartId(
                reservation.provider_upload_id.expect("provider upload")
            )),
        Err(blobyard_contract::StorageError::NotFound)
    );
}

#[tokio::test]
async fn multipart_requests_fail_closed_and_abort_removes_staged_state() {
    let (root, state, project) = fixture();
    let reservation = seed_multipart(&state, &project);
    assert_invalid_requests(&state).await;
    let grants = part_grants(&state, &[1]).await;
    assert_eq!(
        send_raw(&state, "PUT", part_path(&grants, 0), b"ab")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let raw_capability = part_path(&grants, 0)
        .rsplit('/')
        .next()
        .expect("raw capability");
    assert_redacted(&root, raw_capability);
    assert_abort(&state, reservation).await;
}
