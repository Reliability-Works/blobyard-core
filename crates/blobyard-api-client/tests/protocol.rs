//! Endpoint, request preparation, retry, and model accessor contracts.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    AbortUploadRequest, ApiCallError, ApiRequest, BootstrapExchangeRequest, CompleteUploadRequest,
    CompletedPart, DevicePollRequest, DeviceStartRequest, Endpoint, HealthResponse, Page,
    ProjectSummary, RawResponse, RequestDownloadRequest, RequestUploadPartsRequest,
    RequestUploadRequest, RetryAdvice, UploadStatusQuery, WorkspaceSummary,
};
use blobyard_core::{BlobyardError, BlobyardUri, ErrorCode, SecretString, Slug};
use std::error::Error as _;
use std::time::Duration;

#[test]
fn request_builder_validates_bounds_and_redacts_sensitive_fields() {
    let token = SecretString::new("bearer-secret").expect("token");
    let request = ApiRequest::new(Endpoint::RequestAccountExport)
        .with_query("workspace=team".into())
        .with_json(serde_json::json!({ "safe": true }))
        .with_bearer(token)
        .with_idempotency_key("request-key".into())
        .expect("key");
    assert_eq!(request.endpoint(), Endpoint::RequestAccountExport);
    assert_eq!(request.query(), Some("workspace=team"));
    assert_eq!(request.body(), Some(&serde_json::json!({ "safe": true })));
    assert_eq!(
        request.bearer().map(SecretString::expose_secret),
        Some("bearer-secret")
    );
    assert_eq!(request.idempotency_key(), Some("request-key"));
    assert_redacted(&request);

    let empty = ApiRequest::new(Endpoint::Health).with_query(String::new());
    assert_eq!(empty.query(), None);
    assert_eq!(empty.body(), None);
    assert_eq!(empty.bearer(), None);
    assert_eq!(empty.idempotency_key(), None);
}

fn assert_redacted(request: &ApiRequest) {
    let debug = format!("{request:?}");
    for secret in ["bearer-secret", "workspace=team", "request-key"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn request_builder_rejects_invalid_caller_supplied_idempotency_keys() {
    for key in [
        String::new(),
        "x".repeat(129),
        "line\nbreak".into(),
        "slash/key".into(),
        "percent%key".into(),
    ] {
        assert!(
            ApiRequest::new(Endpoint::RequestUpload)
                .with_idempotency_key(key)
                .is_err()
        );
    }
    let exact_openapi_alphabet = ApiRequest::new(Endpoint::RequestUpload)
        .with_idempotency_key("letters.NUMBERS_123:retry-key".into())
        .expect("OpenAPI key alphabet");
    assert_eq!(
        exact_openapi_alphabet.idempotency_key(),
        Some("letters.NUMBERS_123:retry-key")
    );
    let generated =
        ApiRequest::new(Endpoint::PrepareAccountDeletion).with_generated_idempotency_key();
    assert!(
        generated
            .idempotency_key()
            .is_some_and(|key| key.starts_with("blobyard-client-"))
    );
    let deterministic =
        ApiRequest::new(Endpoint::RequestUpload).with_deterministic_idempotency_key([0xab; 32]);
    assert_eq!(
        deterministic.idempotency_key(),
        Some(concat!(
            "blobyard-digest-",
            "abababababababababababababababababababababababababababababababab"
        ))
    );
    let unsupported = ApiRequest::new(Endpoint::CreateApiToken).with_generated_idempotency_key();
    assert_eq!(unsupported.idempotency_key(), None);
    let unsupported =
        ApiRequest::new(Endpoint::CreateApiToken).with_deterministic_idempotency_key([0xcd; 32]);
    assert_eq!(unsupported.idempotency_key(), None);
    assert!(
        ApiRequest::new(Endpoint::CreateApiToken)
            .with_idempotency_key("unsafe-key".into())
            .is_err()
    );
}

#[test]
fn device_requests_encode_the_exact_wire_fields() {
    let start = DeviceStartRequest {
        name: "Release Mac".into(),
        platform: "macos".into(),
        version: "1.0.0".into(),
    };
    assert_eq!(
        start.into_json(),
        serde_json::json!({ "name": "Release Mac", "platform": "macos", "version": "1.0.0" })
    );
    let poll = DevicePollRequest {
        device_code: SecretString::new("device-code-fixture").expect("device code"),
    };
    assert_eq!(
        poll.into_json(),
        serde_json::json!({ "deviceCode": "device-code-fixture" })
    );
}

#[test]
fn bootstrap_exchange_encodes_without_exposing_authority() {
    let request = BootstrapExchangeRequest {
        name: "Blob Yard CLI profile local".into(),
        platform: "macos".into(),
        token: SecretString::new("bootstrap-authority").expect("bootstrap token"),
        version: "0.1.12".into(),
    };
    assert_eq!(
        request.into_json(),
        serde_json::json!({
            "name": "Blob Yard CLI profile local",
            "platform": "macos",
            "token": "bootstrap-authority",
            "version": "0.1.12",
        })
    );
}

#[test]
fn transfer_requests_encode_the_exact_wire_contract() {
    let workspace = Slug::new("team").expect("workspace");
    let project = Slug::new("app").expect("project");
    let request = RequestUploadRequest {
        workspace,
        project,
        path: "builds/app.zip".into(),
        filename: "app.zip".into(),
        size_bytes: 42,
        checksum_sha256: "abc123".into(),
        content_type: "application/zip".into(),
        git_repository: Some("blobyard/blobyard".into()),
        git_commit: Some("a".repeat(40)),
        git_branch: Some("main".into()),
    };
    let upload = request.into_json();
    assert_eq!(upload["sizeBytes"], 42);
    assert_eq!(upload["gitRepository"], "blobyard/blobyard");
    let parts = RequestUploadPartsRequest {
        upload_id: "upload_1".into(),
        part_numbers: vec![1, 2],
    };
    assert_eq!(parts.into_json()["partNumbers"], serde_json::json!([1, 2]));
    let completion = CompleteUploadRequest {
        upload_id: "upload_1".into(),
        parts: vec![CompletedPart {
            part_number: 1,
            etag: "etag-1".into(),
        }],
    };
    assert_eq!(completion.into_json()["parts"][0]["etag"], "etag-1");
    assert_eq!(
        AbortUploadRequest {
            upload_id: "upload_1".into()
        }
        .into_json()["uploadId"],
        "upload_1"
    );
    assert_eq!(
        UploadStatusQuery {
            upload_id: "upload 1".into()
        }
        .into_query(),
        "uploadId=upload+1"
    );
    let uri = "blobyard://team/app/build.zip"
        .parse::<BlobyardUri>()
        .expect("uri");
    assert_eq!(
        RequestDownloadRequest { uri }.into_json()["uri"],
        "blobyard://team/app/build.zip"
    );
}

#[test]
fn upload_request_omits_unknown_provenance() {
    let request = RequestUploadRequest {
        workspace: Slug::new("team").expect("workspace"),
        project: Slug::new("app").expect("project"),
        path: "artifact.bin".into(),
        filename: "artifact.bin".into(),
        size_bytes: 0,
        checksum_sha256: "0".repeat(64),
        content_type: "application/octet-stream".into(),
        git_repository: None,
        git_commit: None,
        git_branch: None,
    };
    let encoded = request.into_json();
    assert!(encoded.get("gitRepository").is_none());
    assert!(encoded.get("gitCommit").is_none());
    assert!(encoded.get("gitBranch").is_none());
}

#[test]
fn read_retry_classification_covers_supported_transient_failures() {
    let read = ApiRequest::new(Endpoint::Health);
    assert_eq!(read.retry_advice(200, None), RetryAdvice::Never);
    assert_eq!(read.retry_advice(500, None), RetryAdvice::SafeRequest);
    assert_eq!(read.retry_advice(408, None), RetryAdvice::SafeRequest);
    assert_eq!(read.retry_advice(425, None), RetryAdvice::SafeRequest);
    assert_eq!(read.retry_advice(502, None), RetryAdvice::SafeRequest);
    assert_eq!(read.retry_advice(503, None), RetryAdvice::SafeRequest);
    assert_eq!(read.retry_advice(504, None), RetryAdvice::SafeRequest);
    assert_eq!(
        read.retry_advice(429, None),
        RetryAdvice::After(Duration::from_secs(1))
    );
    assert_eq!(
        read.retry_advice(429, Some(Duration::from_secs(7))),
        RetryAdvice::After(Duration::from_secs(7))
    );
}

#[test]
fn mutation_retry_classification_requires_durable_idempotency() {
    let unsupported = ApiRequest::new(Endpoint::CreateProject).with_generated_idempotency_key();
    assert_eq!(unsupported.idempotency_key(), None);
    assert_eq!(unsupported.retry_advice(500, None), RetryAdvice::Never);
    let protected = ApiRequest::new(Endpoint::RequestAccountExport)
        .with_idempotency_key("safe-key".into())
        .expect("key");
    assert_eq!(
        protected.retry_advice(500, None),
        RetryAdvice::IdempotentMutation
    );
}

#[test]
fn raw_response_and_call_error_have_safe_public_behavior() {
    let raw = RawResponse::new(429, Some("req_1".into()), b"body".to_vec())
        .with_retry_after(Duration::from_secs(3));
    assert!(format!("{raw:?}").contains("req_1"));

    let error = ApiCallError::new(
        BlobyardError::from_code(ErrorCode::RateLimited),
        RetryAdvice::After(Duration::from_secs(3)),
    );
    assert_eq!(error.error().code(), ErrorCode::RateLimited);
    assert_eq!(
        error.retry_advice(),
        RetryAdvice::After(Duration::from_secs(3))
    );
    assert!(error.source().is_some());
    assert!(error.to_string().contains("RATE_LIMITED"));
    assert_eq!(error.into_error().code(), ErrorCode::RateLimited);
}

#[test]
fn health_and_workspace_model_accessors_are_stable() {
    let health: HealthResponse = serde_json::from_value(serde_json::json!({
        "status": "ok", "version": "1.2.3"
    }))
    .expect("health");
    assert_eq!(health.status(), "ok");
    assert_eq!(health.version(), "1.2.3");

    let workspace: WorkspaceSummary = serde_json::from_value(serde_json::json!({
        "id": "w1", "slug": "team", "name": "Team"
    }))
    .expect("workspace");
    assert_eq!(workspace.id(), "w1");
    assert_eq!(workspace.slug(), &Slug::new("team").expect("slug"));
    assert_eq!(workspace.name(), "Team");
}

#[test]
fn project_and_page_model_accessors_are_stable() {
    let project: ProjectSummary = serde_json::from_value(serde_json::json!({
        "id": "p1", "workspaceSlug": "team", "slug": "app", "name": "App"
    }))
    .expect("project");
    assert_eq!(project.id(), "p1");
    assert_eq!(project.workspace_slug().as_str(), "team");
    assert_eq!(project.slug().as_str(), "app");
    assert_eq!(project.name(), "App");

    let page = Page::new(vec![project], Some("next".into()));
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.next_cursor(), Some("next"));
    assert_eq!(Page::<String>::new(Vec::new(), None).next_cursor(), None);
}
