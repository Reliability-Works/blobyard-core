use super::*;

fn bind_fixture_token_to_other_project(root: &TempDir, state: &AppState) {
    let other = ProjectRecord {
        id: "project_other".to_owned(),
        workspace_id: state.default_workspace.id.clone(),
        slug: slug("other"),
        name: "Other".to_owned(),
    };
    state
        .repository
        .create_project(&other)
        .expect("other project");
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute(
            "UPDATE api_tokens SET project_id = ?2 WHERE id = ?1",
            ["token_fixture", other.id.as_str()],
        )
        .expect("bind token to other project");
}

#[tokio::test]
async fn project_bound_tokens_conceal_every_foreign_project_transfer_path() {
    let (root, state, _project) = fixture();
    bind_fixture_token_to_other_project(&root, &state);

    let mut foreign_request = request("foreign/path");
    foreign_request.project = slug("project");
    let denied = send_json(
        &state,
        "POST",
        "/v1/uploads/request",
        serde_json::to_value(&foreign_request).expect("request value"),
        Some("foreign-project"),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);

    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: state.default_workspace.id.clone(),
        slug: slug("project"),
        name: "Project".to_owned(),
    };
    let capability = SecretString::new("foreign-capability").expect("capability");
    let input = crate::transfer_grants::reservation_input(
        &foreign_request,
        &project,
        "upload_foreign",
        &capability,
        u64::try_from(i64::MAX).expect("maximum expiry"),
        blobyard_contract::ObjectSource::Cli,
    );
    state
        .repository
        .reserve_upload(&input)
        .expect("foreign reservation");
    for (method, path, body) in [
        (
            "GET",
            "/v1/uploads/status?uploadId=upload_foreign",
            serde_json::json!({}),
        ),
        (
            "POST",
            "/v1/uploads/complete",
            serde_json::json!({ "uploadId": "upload_foreign", "parts": [] }),
        ),
        (
            "POST",
            "/v1/uploads/abort",
            serde_json::json!({ "uploadId": "upload_foreign" }),
        ),
    ] {
        let denied = send_json(&state, method, path, body, None).await;
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn upload_abort_conceals_corrupt_storage_keys_without_deleting_storage() {
    let (_root, mut state, project) = fixture();
    let capability = SecretString::new("capability").expect("capability");
    let input = crate::transfer_grants::reservation_input(
        &request("valid/path"),
        &project,
        "upload_abort",
        &capability,
        u64::try_from(i64::MAX).expect("maximum expiry"),
        blobyard_contract::ObjectSource::Cli,
    );
    state
        .repository
        .reserve_upload(&input)
        .expect("reservation");

    let calls = Arc::new(AtomicUsize::new(0));
    state.storage = delete_counting_storage::new(Arc::clone(&state.storage), Arc::clone(&calls));
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::corrupting(
        inner,
        Corruption::AbortedStorageKey,
    ));
    let response = send_json(
        &state,
        "POST",
        "/v1/uploads/abort",
        serde_json::json!({ "uploadId": "upload_abort" }),
        None,
    )
    .await;

    assert_internal(response).await;
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}
