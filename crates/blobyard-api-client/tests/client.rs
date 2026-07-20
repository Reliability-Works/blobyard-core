//! Strict envelope decoding over a deterministic one-shot transport.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    ApiCallError, ApiClient, ApiDeployment, ApiRequest, EmptyResponse, Endpoint, RawResponse,
    RetryAdvice, Transport, TransportFuture,
};
use blobyard_core::{BlobyardError, ErrorCode};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct QueueTransport {
    responses: Mutex<VecDeque<Result<RawResponse, ApiCallError>>>,
    calls: Mutex<usize>,
}

impl QueueTransport {
    fn new(response: Result<RawResponse, ApiCallError>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from([response])),
            calls: Mutex::new(0),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().expect("calls lock")
    }
}

impl Transport for QueueTransport {
    fn send<'a>(&'a self, _request: &'a ApiRequest) -> TransportFuture<'a> {
        Box::pin(async move {
            *self.calls.lock().expect("calls lock") += 1;
            self.responses
                .lock()
                .expect("response lock")
                .pop_front()
                .expect("queued response")
        })
    }
}

fn response(status: u16, header_id: Option<&str>, body: &str) -> RawResponse {
    RawResponse::new(
        status,
        header_id.map(ToOwned::to_owned),
        body.as_bytes().to_vec(),
    )
}

async fn execute(
    response: Result<RawResponse, ApiCallError>,
) -> Result<blobyard_api_client::ApiSuccess<EmptyResponse>, ApiCallError> {
    ApiClient::new(Arc::new(QueueTransport::new(response)))
        .execute(ApiRequest::new(Endpoint::Health))
        .await
}

#[tokio::test]
async fn decodes_success_and_exposes_only_safe_metadata() {
    let transport = Arc::new(QueueTransport::new(Ok(response(
        200,
        Some("req_success"),
        r#"{"ok":true,"data":{},"requestId":"req_success"}"#,
    ))));
    let client = ApiClient::new(transport.clone());
    assert!(format!("{client:?}").contains("ApiClient"));
    let success = client
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::Health))
        .await
        .expect("success");
    assert_eq!(success.data(), &EmptyResponse::default());
    assert_eq!(success.request_id(), "req_success");
    assert_eq!(success.into_data(), EmptyResponse::default());
    assert_eq!(transport.calls(), 1);
}

#[tokio::test]
async fn maps_api_errors_to_stable_copy_and_ignores_provider_message() {
    let raw = response(
        429,
        Some("req_rate"),
        r#"{"ok":false,"error":{"code":"RATE_LIMITED","message":"secret provider detail","details":{"internal":"hidden"}},"requestId":"req_rate"}"#,
    )
    .with_retry_after(std::time::Duration::from_secs(8));
    let error = execute(Ok(raw)).await.expect_err("rate limit");
    assert_eq!(error.error().code(), ErrorCode::RateLimited);
    assert_eq!(
        error.error().message(),
        ErrorCode::RateLimited.default_message()
    );
    assert!(!error.to_string().contains("secret provider detail"));
    assert_eq!(error.error().request_id(), Some("req_rate"));
    assert_eq!(
        error.retry_advice(),
        RetryAdvice::After(std::time::Duration::from_secs(8))
    );
}

#[tokio::test]
async fn rejects_missing_invalid_or_mismatched_request_identifiers() {
    let cases = [
        response(200, None, r#"{"ok":true,"data":{},"requestId":"req"}"#),
        response(200, Some(""), r#"{"ok":true,"data":{},"requestId":""}"#),
        response(
            200,
            Some("bad id"),
            r#"{"ok":true,"data":{},"requestId":"bad id"}"#,
        ),
        response(
            200,
            Some(&"x".repeat(129)),
            r#"{"ok":true,"data":{},"requestId":"x"}"#,
        ),
        response(
            200,
            Some("req_header"),
            r#"{"ok":true,"data":{},"requestId":"req_body"}"#,
        ),
    ];
    for raw in cases {
        let error = execute(Ok(raw)).await.expect_err("invalid request id");
        assert_eq!(error.error().code(), ErrorCode::ProviderUnavailable);
    }
}

#[tokio::test]
async fn rejects_malformed_unknown_and_inconsistent_envelopes() {
    let cases = [
        "not-json",
        r#"{"ok":false,"error":{"code":"UNKNOWN","message":"x"},"requestId":"req"}"#,
        r#"{"ok":true,"requestId":"req"}"#,
        r#"{"ok":true,"data":{},"error":{"code":"INTERNAL_ERROR","message":"x"},"requestId":"req"}"#,
        r#"{"ok":false,"data":{},"error":{"code":"NOT_FOUND","message":"x"},"requestId":"req"}"#,
        r#"{"ok":false,"requestId":"req"}"#,
        r#"{"ok":true,"data":{},"requestId":"req","extra":true}"#,
    ];
    for body in cases {
        let error = execute(Ok(response(200, Some("req"), body)))
            .await
            .expect_err("invalid envelope");
        assert_eq!(error.error().code(), ErrorCode::ProviderUnavailable);
    }
    let inconsistent_status = execute(Ok(response(
        500,
        Some("req"),
        r#"{"ok":true,"data":{},"requestId":"req"}"#,
    )))
    .await
    .expect_err("status mismatch");
    assert_eq!(
        inconsistent_status.error().code(),
        ErrorCode::ProviderUnavailable
    );
}

#[tokio::test]
async fn propagates_transport_failure_without_retrying() {
    let expected = ApiCallError::new(
        BlobyardError::from_code(ErrorCode::NetworkError),
        RetryAdvice::SafeRequest,
    );
    let transport = Arc::new(QueueTransport::new(Err(expected.clone())));
    let client = ApiClient::new(transport.clone());
    let actual = client
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::Health))
        .await
        .expect_err("transport failure");
    assert_eq!(actual, expected);
    assert_eq!(transport.calls(), 1);
}

#[tokio::test]
async fn self_hosted_clients_reject_hosted_operations_before_transport() {
    let transport = Arc::new(QueueTransport::new(Ok(response(
        200,
        Some("must_not_send"),
        r#"{"ok":true,"data":{},"requestId":"must_not_send"}"#,
    ))));
    let client = ApiClient::for_deployment(transport.clone(), ApiDeployment::SelfHosted);
    let error = client
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::CreateBillingPortal))
        .await
        .expect_err("hosted extension must be unavailable");
    assert_eq!(error.error().code(), ErrorCode::OperationUnsupported);
    assert_eq!(error.retry_advice(), RetryAdvice::Never);
    assert_eq!(transport.calls(), 0);
}

#[tokio::test]
async fn self_hosted_clients_send_core_operations() {
    let transport = Arc::new(QueueTransport::new(Ok(response(
        200,
        Some("req_core"),
        r#"{"ok":true,"data":{},"requestId":"req_core"}"#,
    ))));
    let client = ApiClient::for_deployment(transport.clone(), ApiDeployment::SelfHosted);
    let success = client
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::Health))
        .await
        .expect("core operation");
    assert_eq!(success.request_id(), "req_core");
    assert_eq!(transport.calls(), 1);
}

#[tokio::test]
async fn cloud_clients_reject_self_hosted_bootstrap_before_transport() {
    let transport = Arc::new(QueueTransport::new(Ok(response(
        200,
        Some("must_not_send"),
        r#"{"ok":true,"data":{},"requestId":"must_not_send"}"#,
    ))));
    let client = ApiClient::for_deployment(transport.clone(), ApiDeployment::Cloud);
    let error = client
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::ExchangeBootstrapToken))
        .await
        .expect_err("self-hosted bootstrap must be unavailable in Cloud");
    assert_eq!(error.error().code(), ErrorCode::OperationUnsupported);
    assert_eq!(error.retry_advice(), RetryAdvice::Never);
    assert_eq!(transport.calls(), 0);
}
