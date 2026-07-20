#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::{
    ABC_SHA256, ABCDEF_SHA256, Endpoint, ErrorCode, Fixture, api_failure, completion, empty_reply,
    etag_reply, execute_error, part_grants, reservation, signed_server, upload_command,
};

#[tokio::test]
async fn multipart_upload_is_concurrent_and_resumable() {
    let source_root = tempfile::tempdir().expect("source root");
    let source = source_root.path().join("large.bin");
    std::fs::write(&source, b"abcdef").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    failed_resume_attempt(&source_text).await;
    complete_resume(&source_text).await;
}

async fn failed_resume_attempt(source_text: &str) {
    let replies = vec![
        empty_reply("503 Unavailable"),
        empty_reply("503 Unavailable"),
        empty_reply("503 Unavailable"),
    ];
    let (failing_url, failed_storage) = signed_server(replies).await;
    let first = Fixture::new(
        &upload_command(source_text),
        vec![
            reservation(
                "multipart",
                &failing_url,
                Some(8 * 1024 * 1024),
                "upload_resume",
            ),
            part_grants(&failing_url, &[1]),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&first).await, ErrorCode::StorageError);
    failed_storage.await.expect("failed storage");
}

async fn complete_resume(source_text: &str) {
    let (url, storage) = signed_server(vec![etag_reply("etag-resumed")]).await;
    let second = Fixture::new(
        &upload_command(source_text),
        vec![
            super::ok(
                serde_json::json!({ "state": "pending", "completedParts": [] }),
                "req_status",
            ),
            part_grants(&url, &[1]),
            completion(6, ABCDEF_SHA256),
        ],
        Some("ci-token"),
        None,
    );
    second
        .runner
        .execute(&second.command)
        .await
        .expect("resumed upload");
    let requests = second.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::UploadStatus);
    assert_eq!(
        requests[2].body().expect("complete")["parts"][0]["etag"],
        "etag-resumed"
    );
    storage.await.expect("storage");
}

#[tokio::test]
async fn invalid_storage_grants_fail_before_signed_transfer() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("file.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let single = Fixture::new(
        &upload_command(&source_text),
        vec![reservation("single", "", None, "upload_bad")],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&single).await, ErrorCode::ProviderUnavailable);
    let multipart = Fixture::new(
        &upload_command(&source_text),
        vec![reservation("multipart", "", Some(1), "upload_bad")],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&multipart).await,
        ErrorCode::ProviderUnavailable
    );
    let missing_part_size = Fixture::new(
        &upload_command(&source_text),
        vec![reservation("multipart", "", None, "upload_bad")],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&missing_part_size).await,
        ErrorCode::ProviderUnavailable
    );
}

#[tokio::test]
async fn changed_source_discards_stale_resume_state() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("changed.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    std::fs::write(&source, b"changed").expect("changed source");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![
            reservation("single", &url, None, "upload_fresh"),
            completion(7, ABC_SHA256),
        ],
        Some("ci-token"),
        None,
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("fresh upload");
    assert_eq!(
        fixture.transport.requests()[0].endpoint(),
        Endpoint::RequestUpload
    );
    storage.await.expect("storage");
}

#[tokio::test]
async fn missing_or_forbidden_resume_status_is_handled_safely() {
    let missing_root = tempfile::tempdir().expect("missing root");
    let missing_source = missing_root.path().join("missing.bin");
    std::fs::write(&missing_source, b"abc").expect("missing source");
    let missing_text = missing_source.to_string_lossy().into_owned();
    leave_resume_state(&missing_text).await;
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let missing = Fixture::new(
        &upload_command(&missing_text),
        vec![
            api_failure(ErrorCode::NotFound, "req_missing"),
            reservation("single", &url, None, "upload_new"),
            completion(3, ABC_SHA256),
        ],
        Some("ci-token"),
        None,
    );
    missing
        .runner
        .execute(&missing.command)
        .await
        .expect("new reservation");
    storage.await.expect("storage");
    forbidden_resume_status().await;
}

#[tokio::test]
async fn corrupt_resume_state_fails_before_api_access() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("corrupt.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    std::fs::write(resume_path(root.path()), b"not-json").expect("corrupt state");
    let fixture = Fixture::new(
        &upload_command(&source_text),
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&fixture).await, ErrorCode::StorageError);
}

#[cfg(unix)]
#[tokio::test]
async fn stale_resume_removal_reports_read_only_storage() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("stale.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    std::fs::write(&source, b"changed").expect("changed");
    lock(root.path());
    let fixture = Fixture::new(
        &upload_command(&source_text),
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let error = execute_error(&fixture).await;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).expect("unlock");
    assert_eq!(error, ErrorCode::StorageError);
}

#[cfg(unix)]
#[tokio::test]
async fn status_reconciliation_reports_read_only_resume_state() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("status.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    lock(root.path());
    let status = super::ok(
        serde_json::json!({ "state": "pending", "completedParts": [] }),
        "req_status",
    );
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![status],
        Some("ci-token"),
        None,
    );
    let error = execute_error(&fixture).await;
    unlock(root.path());
    assert_eq!(error, ErrorCode::StorageError);
}

#[cfg(unix)]
#[tokio::test]
async fn missing_status_reports_failed_read_only_cleanup() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("missing-status.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    lock(root.path());
    let fixture = Fixture::new(
        &upload_command(&source_text),
        vec![api_failure(ErrorCode::NotFound, "req_missing")],
        Some("ci-token"),
        None,
    );
    let error = execute_error(&fixture).await;
    unlock(root.path());
    assert_eq!(error, ErrorCode::StorageError);
}

async fn forbidden_resume_status() {
    let root = tempfile::tempdir().expect("denied root");
    let source = root.path().join("denied.bin");
    std::fs::write(&source, b"abc").expect("denied source");
    let source_text = source.to_string_lossy().into_owned();
    leave_resume_state(&source_text).await;
    let denied = Fixture::new(
        &upload_command(&source_text),
        vec![api_failure(ErrorCode::Forbidden, "req_denied")],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&denied).await, ErrorCode::Forbidden);
}

async fn leave_resume_state(source: &str) {
    let replies = vec![
        empty_reply("503 Unavailable"),
        empty_reply("503 Unavailable"),
        empty_reply("503 Unavailable"),
    ];
    let (url, storage) = signed_server(replies).await;
    let fixture = Fixture::new(
        &upload_command(source),
        vec![
            reservation("multipart", &url, Some(8 * 1024 * 1024), "upload_old"),
            part_grants(&url, &[1]),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&fixture).await, ErrorCode::StorageError);
    storage.await.expect("storage");
}

fn resume_path(root: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(root)
        .expect("state directory")
        .map(|entry| entry.expect("entry").path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".blobyard-resume-"))
        })
        .expect("resume state")
}

#[cfg(unix)]
fn lock(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500)).expect("lock");
}

#[cfg(unix)]
fn unlock(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).expect("unlock");
}
