#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::{
    ABC_SHA256, ErrorCode, Fixture, SignedReply, api_failure, download_grant, etag_reply,
    execute_error, part_grants, reservation, signed_server, upload_command,
};

#[tokio::test]
async fn upload_validation_fails_before_api_access() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let missing_scope = Fixture::new(
        &["blobyard", "upload", &source_text],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&missing_scope).await,
        ErrorCode::InvalidRequest
    );

    let missing = root.path().join("missing").to_string_lossy().into_owned();
    let missing_source = Fixture::new(
        &upload_command(&missing),
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&missing_source).await,
        ErrorCode::StorageError
    );
}

#[tokio::test]
async fn multipart_api_failures_propagate_without_false_success() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let grants = Fixture::new(
        &upload_command(&source_text),
        vec![
            reservation("multipart", "", Some(8 * 1024 * 1024), "upload_parts"),
            api_failure(ErrorCode::Forbidden, "req_parts_denied"),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&grants).await, ErrorCode::Forbidden);

    let complete_source = root.path().join("complete.bin");
    std::fs::write(&complete_source, b"abc").expect("complete source");
    let complete_text = complete_source.to_string_lossy().into_owned();
    let (url, storage) = signed_server(vec![etag_reply("etag-complete")]).await;
    let complete = Fixture::new(
        &upload_command(&complete_text),
        vec![
            reservation("multipart", &url, Some(8 * 1024 * 1024), "upload_complete"),
            part_grants(&url, &[1]),
            api_failure(ErrorCode::Conflict, "req_complete_conflict"),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&complete).await, ErrorCode::Conflict);
    storage.await.expect("storage");
}

#[tokio::test]
async fn single_reservation_requires_a_signed_url() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let response = super::ok(
        serde_json::json!({
            "uploadId": "upload_missing_url",
            "strategy": "single",
            "uploadUrl": null,
            "headers": [],
            "partSizeBytes": null,
            "expiresAt": "2030-01-01T00:00:00Z"
        }),
        "req_reserve",
    );
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![response],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&fixture).await,
        ErrorCode::ProviderUnavailable
    );
}

#[cfg(unix)]
#[tokio::test]
async fn multipart_resume_creation_reports_read_only_storage() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o500)).expect("lock");
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![reservation(
            "multipart",
            "",
            Some(8 * 1024 * 1024),
            "upload_read_only",
        )],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&fixture).await, ErrorCode::StorageError);
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).expect("unlock");
}

#[cfg(unix)]
#[tokio::test]
async fn completed_part_reports_read_only_resume_state() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let locked_root = root.path().to_path_buf();
    let (url, storage) = super::signed_server_with_action(etag_reply("etag"), move || {
        std::fs::set_permissions(&locked_root, std::fs::Permissions::from_mode(0o500))
            .expect("lock");
    })
    .await;
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![
            reservation("multipart", &url, Some(8 * 1024 * 1024), "upload_part"),
            part_grants(&url, &[1]),
        ],
        Some("ci-token"),
        None,
    );
    let error = execute_error(&fixture).await;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).expect("unlock");
    assert_eq!(error, ErrorCode::StorageError);
    storage.await.expect("storage");
}

#[tokio::test]
async fn download_reports_atomic_placement_failure() {
    let root = tempfile::tempdir().expect("root");
    let destination = root.path().join("artifact.bin");
    let action_destination = destination.clone();
    let reply = SignedReply {
        status: "200 OK",
        headers: Vec::new(),
        body: b"abc".to_vec(),
    };
    let (url, storage) = super::signed_server_with_action(reply, move || {
        std::fs::create_dir(&action_destination).expect("destination race");
    })
    .await;
    let output = destination.to_string_lossy().into_owned();
    let fixture = super::download_fixture(
        "blobyard://team/app/artifact.bin",
        &output,
        vec![download_grant(&url, ABC_SHA256)],
    );
    assert_eq!(execute_error(&fixture).await, ErrorCode::StorageError);
    storage.await.expect("storage");
}
