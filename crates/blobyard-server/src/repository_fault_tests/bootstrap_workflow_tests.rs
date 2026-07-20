use super::send;
use axum::{Router, http::StatusCode};
use serde_json::{Value, json};

pub(super) async fn run(router: &Router) -> Result<String, (String, StatusCode)> {
    let path = "/v1/bootstrap/exchange";
    let body = serde_json::to_vec(&json!({
        "name": "Fixture",
        "platform": "test",
        "token": "bootstrap",
        "version": "0.0.0-test"
    }))
    .expect("bootstrap JSON");
    let (status, response) = send(router, "POST", path, body, None, None).await;
    if !status.is_success() {
        return Err((path.to_owned(), status));
    }
    let response: Value = serde_json::from_slice(&response).expect("bootstrap response");
    Ok(response["data"]["accessToken"]
        .as_str()
        .expect("access token")
        .to_owned())
}
