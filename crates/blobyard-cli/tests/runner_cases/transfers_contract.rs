#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::{
    ABC_SHA256, Endpoint, ErrorCode, Fixture, api_failure, completion, empty_reply, execute_error,
    reservation, signed_server, upload_command,
};
use std::path::Path;
use std::process::Command;

#[tokio::test]
async fn unchanged_uploads_use_the_same_deterministic_key() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let source_text = source.to_string_lossy().into_owned();
    let first = denied_reservation(&source_text);
    let second = denied_reservation(&source_text);
    assert_eq!(execute_error(&first).await, ErrorCode::Forbidden);
    assert_eq!(execute_error(&second).await, ErrorCode::Forbidden);
    let first_key = first.transport.requests()[0]
        .idempotency_key()
        .expect("first key")
        .to_owned();
    let second_key = second.transport.requests()[0]
        .idempotency_key()
        .expect("second key")
        .to_owned();
    assert_eq!(first_key, second_key);
}

#[tokio::test]
async fn upload_attaches_only_safe_git_provenance() {
    let root = tempfile::tempdir().expect("root");
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "user.name", "Blobyard Test"]);
    git(root.path(), &["config", "user.email", "test@invalid"]);
    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://user:secret@github.com/blobyard/private.git",
        ],
    );
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    git(root.path(), &["add", "artifact.bin"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    git(root.path(), &["branch", "-M", "main"]);
    let fixture = denied_reservation(&source.to_string_lossy());
    assert_eq!(execute_error(&fixture).await, ErrorCode::Forbidden);
    let requests = fixture.transport.requests();
    let body = requests[0].body().expect("body");
    assert_eq!(body["gitRepository"], "blobyard/private");
    assert_eq!(body["gitBranch"], "main");
    assert_eq!(body["gitCommit"].as_str().expect("commit").len(), 40);
    assert!(!body.to_string().contains("secret"));
}

#[tokio::test]
async fn directory_upload_preserves_sorted_relative_paths() {
    let root = tempfile::tempdir().expect("root");
    let tree = root.path().join("tree");
    std::fs::create_dir_all(tree.join("nested")).expect("tree");
    std::fs::write(tree.join("z.bin"), b"abc").expect("z");
    std::fs::write(tree.join("nested/a.bin"), b"abc").expect("a");
    let (url, storage) = signed_server(vec![empty_reply("200 OK"), empty_reply("200 OK")]).await;
    let source = tree.to_string_lossy().into_owned();
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "upload",
            &source,
            "--path",
            "builds",
        ],
        vec![
            reservation("single", &url, None, "upload_a"),
            completion(3, ABC_SHA256),
            reservation("single", &url, None, "upload_z"),
            completion(3, ABC_SHA256),
        ],
        Some("ci-token"),
        None,
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("upload");
    let paths = fixture
        .transport
        .requests()
        .into_iter()
        .filter(|request| request.endpoint() == Endpoint::RequestUpload)
        .map(|request| request.body().expect("body")["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["builds/nested/a.bin", "builds/z.bin"]);
    assert_eq!(storage.await.expect("storage").len(), 2);
}

fn denied_reservation(source: &str) -> Fixture {
    Fixture::new(
        &upload_command(source),
        vec![api_failure(ErrorCode::Forbidden, "req_denied")],
        Some("ci-token"),
        None,
    )
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .status()
        .expect("git");
    assert!(status.success());
}
