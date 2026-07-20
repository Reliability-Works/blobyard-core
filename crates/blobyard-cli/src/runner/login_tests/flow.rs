use super::support::{
    Fixture, PortState, RecordingPort, approved_response, poll_response, start_response,
};
use blobyard_api_client::{Endpoint, RawResponse};
use std::sync::Arc;
use std::time::Duration;

fn rate_limited_response() -> RawResponse {
    super::support::api_failure(blobyard_core::ErrorCode::RateLimited, 429, "req_rate")
        .with_retry_after(Duration::from_secs(7))
}

#[tokio::test]
async fn login_polls_slowly_opens_browser_and_persists_only_refresh_token() {
    let port = Arc::new(RecordingPort::default());
    let fixture = Fixture::new(
        &["blobyard", "login", "--name", "Release Mac"],
        vec![
            start_response(5, "https://blobyard.com/cli/activate"),
            poll_response("pending", None, "req_pending"),
            poll_response("slow_down", None, "req_slow"),
            approved_response(),
        ],
    )
    .with_port(port.clone());
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("device login");

    assert_eq!(result_status(result), "signed_in");
    assert_eq!(
        fixture.store.token().as_deref(),
        Some("refresh-token-fixture")
    );
    assert_eq!(fixture.store.saves(), 1);
    assert_port_state(&port.state());
    assert_requests(&fixture.transport.requests());
}

fn result_status(result: crate::CommandResult) -> String {
    let rendered = crate::OutputRenderer::new(
        crate::OutputOptions::from_flags(&crate::GlobalArgs {
            api_url: None,
            web_yard_origin: None,
            profile: None,
            json: true,
            project: None,
            quiet: false,
            retry_key: None,
            verbose: false,
            workspace: None,
        }),
        crate::Diagnostics::default(),
    )
    .success(result);
    let value: serde_json::Value = serde_json::from_str(&rendered.stdout).expect("result json");
    value["data"]["status"].as_str().expect("status").to_owned()
}

fn assert_port_state(state: &PortState) {
    assert_eq!(
        state.instructions,
        [(
            "https://blobyard.com/cli/activate".into(),
            "ABCD-2345".into()
        )]
    );
    assert_eq!(state.opened.len(), 1);
    assert!(state.opened[0].contains("user_code=ABCD-2345"));
    assert_eq!(state.waits, [5, 5, 10].map(Duration::from_secs));
}

fn assert_requests(requests: &[blobyard_api_client::ApiRequest]) {
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].endpoint(), Endpoint::DeviceStart);
    assert_eq!(
        requests[0].body().and_then(|body| body["name"].as_str()),
        Some("Release Mac")
    );
    for request in &requests[1..] {
        assert_eq!(request.endpoint(), Endpoint::DevicePoll);
        assert_eq!(request.idempotency_key(), None);
        assert_eq!(
            request.body().and_then(|body| body["deviceCode"].as_str()),
            Some("device-code-fixture")
        );
    }
}

#[tokio::test]
async fn login_does_not_retry_a_poll_without_durable_replay() {
    let port = Arc::new(RecordingPort::default());
    let fixture = Fixture::new(
        &["blobyard", "login"],
        vec![
            start_response(1, "https://blobyard.com/cli/activate"),
            rate_limited_response(),
        ],
    )
    .with_port(port);
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("ambiguous poll failure");

    assert_eq!(error.code(), blobyard_core::ErrorCode::RateLimited);
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].endpoint(), Endpoint::DevicePoll);
    assert_eq!(requests[1].idempotency_key(), None);
}

#[tokio::test]
async fn no_open_uses_the_default_name_and_skips_browser_launch() {
    let port = Arc::new(RecordingPort::default());
    let fixture = Fixture::new(
        &["blobyard", "login", "--no-open"],
        vec![
            start_response(1, "https://blobyard.com/cli/activate"),
            approved_response(),
        ],
    )
    .with_port(port.clone());
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("device login");

    assert!(port.state().opened.is_empty());
    assert_eq!(
        fixture.transport.requests()[0]
            .body()
            .and_then(|body| body["name"].as_str()),
        Some("Blobyard CLI")
    );
}
