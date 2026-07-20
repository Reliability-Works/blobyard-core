use super::support::{
    Fixture, RecordingPort, api_failure, approved_response, poll_response, start_response,
};
use blobyard_api_client::{DevicePollResponse, DevicePollState};
use blobyard_core::ErrorCode;
use std::sync::Arc;

#[tokio::test]
async fn denied_expired_and_inconsistent_poll_states_fail_safely() {
    for (status, tokens, expected) in poll_failures() {
        let fixture = Fixture::new(
            &["blobyard", "login", "--no-open"],
            vec![
                start_response(1, "https://blobyard.com/cli/activate"),
                poll_response(status, tokens, "req_poll"),
            ],
        )
        .with_port(Arc::new(RecordingPort::default()));
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("login must fail")
                .code(),
            expected
        );
    }
}

fn poll_failures() -> Vec<(&'static str, Option<serde_json::Value>, ErrorCode)> {
    let tokens = serde_json::json!({
        "accessToken": "access",
        "refreshToken": "refresh",
        "expiresInSeconds": 900
    });
    vec![
        ("denied", None, ErrorCode::Forbidden),
        ("expired", None, ErrorCode::TokenExpired),
        ("pending", Some(tokens), ErrorCode::ProviderUnavailable),
        ("approved", None, ErrorCode::ProviderUnavailable),
    ]
}

#[tokio::test]
async fn invalid_start_and_remote_failures_do_not_persist_credentials() {
    let responses = [
        vec![start_response(0, "https://blobyard.com/cli/activate")],
        vec![start_response(1, "not a URL")],
        vec![api_failure(
            ErrorCode::RateLimited,
            400,
            "req_start_failure",
        )],
        vec![
            start_response(1, "https://blobyard.com/cli/activate"),
            api_failure(ErrorCode::ProviderUnavailable, 500, "req_poll_failure"),
        ],
    ];
    for queued in responses {
        let fixture = Fixture::new(&["blobyard", "login", "--no-open"], queued)
            .with_port(Arc::new(RecordingPort::default()));
        assert!(fixture.runner.execute(&fixture.command).await.is_err());
        assert_eq!(fixture.store.saves(), 0);
    }
}

#[tokio::test]
async fn approved_login_reports_local_credential_store_failure() {
    let fixture = Fixture::new(
        &["blobyard", "login", "--no-open"],
        vec![
            start_response(1, "https://blobyard.com/cli/activate"),
            approved_response(),
        ],
    )
    .with_port(Arc::new(RecordingPort::default()));
    fixture.store.fail_save();
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("save must fail")
            .code(),
        ErrorCode::InternalError
    );
}

#[test]
fn finish_login_rejects_an_approved_response_without_tokens() {
    let fixture = Fixture::new(&["blobyard", "login", "--no-open"], Vec::new());
    let response = DevicePollResponse {
        status: DevicePollState::Approved,
        tokens: None,
    };
    assert_eq!(
        fixture
            .runner
            .finish_login(&response, "req_invalid")
            .expect_err("tokens are required")
            .code(),
        ErrorCode::ProviderUnavailable
    );
}
