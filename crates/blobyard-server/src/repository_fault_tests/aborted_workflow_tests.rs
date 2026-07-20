use super::{json_request, upload_status};
use axum::{Router, http::StatusCode};
use serde_json::{Value, json};

pub(super) async fn run(router: &Router, token: &str) -> Result<(), (String, StatusCode)> {
    let upload = json_request(
        router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "default", "project": "fixture", "path": "aborted.txt",
            "filename": "aborted.txt", "sizeBytes": 0,
            "checksumSha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "contentType": "text/plain"
        })),
        Some("aborted-upload"),
        token,
    )
    .await?;
    abort(router, &upload, token).await
}

async fn abort(router: &Router, upload: &Value, token: &str) -> Result<(), (String, StatusCode)> {
    let upload_id = upload["data"]["uploadId"].as_str().expect("upload ID");
    json_request(
        router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": upload_id })),
        None,
        token,
    )
    .await?;
    upload_status(router, upload_id, token).await?;
    Ok(())
}
