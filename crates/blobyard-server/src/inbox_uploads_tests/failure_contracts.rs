use super::*;
use crate::{
    Repository, inbox_upload_auth::InboxGuest, repository_fault_tests::FaultingRepository,
    test_support::multipart_storage::MultipartStorage,
};
use axum::response::IntoResponse;
use blobyard_api_client::{CompleteUploadRequest, CompletedPart};
use std::sync::Arc;

#[path = "failure_contracts/completion_abort.rs"]
mod completion_abort;

fn status<T>(result: Result<T, ApiError>) -> StatusCode {
    result
        .err()
        .expect("operation failure")
        .into_response()
        .status()
}

fn upload_request(filename: &str, size: u64) -> RequestUploadRequest {
    serde_json::from_slice(&upload_body(filename, size, &hash("x"))).expect("upload request")
}

async fn inbox_guest(fixture: &test_seams::TransferFixture) -> InboxGuest {
    let (_id, token) = create_inbox(fixture, "Failure intake").await;
    let capability_hash = hash(&token);
    let now = grants::now_ms().expect("clock");
    let inbox = fixture
        .state
        .repository
        .inbox_by_capability(&capability_hash, now)
        .expect("inbox");
    InboxGuest {
        capability_hash,
        fingerprint_hash: hash("failure-peer"),
        inbox,
    }
}

fn faulted(state: &AppState, failure_index: usize) -> AppState {
    let mut faulted = state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    faulted.repository = Arc::new(FaultingRepository::new(inner, failure_index));
    faulted
}

fn upload_id(guest: &InboxGuest, idempotency: &str) -> String {
    let identity = format!("{}:{}", guest.inbox.id, guest.fingerprint_hash);
    grants::stable_upload_id(&identity, idempotency)
}

async fn assert_issue_repository_failures() {
    for failure_index in 0..=2 {
        let fixture = test_seams::fixture(&["inbox:manage"]);
        let guest = inbox_guest(&fixture).await;
        let now = guest.inbox.created_at_ms + 1;
        assert_eq!(
            status(issue_at(
                &faulted(&fixture.state, failure_index),
                &guest,
                &upload_request("file.txt", 1),
                "repository-failure",
                now,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

fn assert_invalid_issue_contracts(
    fixture: &test_seams::TransferFixture,
    guest: &InboxGuest,
    now: u64,
) {
    let mut invalid_content_type = upload_request("file.txt", 1);
    invalid_content_type.content_type = "invalid\ncontent-type".to_owned();
    assert_eq!(
        status(issue_at(
            &fixture.state,
            guest,
            &invalid_content_type,
            "invalid-content-type",
            now,
        )),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(issue_at(
            &fixture.state,
            guest,
            &upload_request("file.txt", 1),
            "expiry-overflow",
            u64::MAX,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid_origin = fixture.state.clone();
    invalid_origin.public_origin = "invalid\norigin".to_owned();
    assert_eq!(
        status(issue_at(
            &invalid_origin,
            guest,
            &upload_request("file.txt", 1),
            "invalid-origin",
            now,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        status(issue_at(
            &fixture.state,
            guest,
            &upload_request("too-large.bin", grants::MULTIPART_PART_BYTES * 10_000 + 1),
            "too-many-parts",
            now,
        )),
        StatusCode::BAD_REQUEST
    );
}

fn assert_issue_provider_failure(
    fixture: &test_seams::TransferFixture,
    guest: &InboxGuest,
    now: u64,
) {
    let mut unavailable_storage = fixture.state.clone();
    unavailable_storage.storage = Arc::new(MultipartStorage::unavailable());
    assert_eq!(
        status(issue_at(
            &unavailable_storage,
            guest,
            &upload_request("multipart.bin", grants::SINGLE_UPLOAD_LIMIT_BYTES + 1),
            "provider-unavailable",
            now,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn inbox_issue_fails_before_granting_on_each_invalid_or_unavailable_dependency() {
    assert_issue_repository_failures().await;
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    assert_invalid_issue_contracts(&fixture, &guest, now);
    assert_issue_provider_failure(&fixture, &guest, now);
}

fn assert_renewal_repository_failures(
    fixture: &test_seams::TransferFixture,
    guest: &InboxGuest,
    request: &RequestUploadRequest,
    expired: u64,
) {
    for failure_index in [3, 4] {
        assert_eq!(
            status(issue_at(
                &faulted(&fixture.state, failure_index),
                guest,
                request,
                "renew-failure",
                expired,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn inbox_issue_replays_only_the_same_request_and_renews_expired_grants() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    let original = upload_request("original.txt", 1);
    let _ = issue_at(&fixture.state, &guest, &original, "mismatch", now).expect("first grant");
    assert_eq!(
        status(issue_at(
            &fixture.state,
            &guest,
            &upload_request("changed.txt", 1),
            "mismatch",
            now,
        )),
        StatusCode::CONFLICT
    );

    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    let request = upload_request("renew.txt", 1);
    let _ = issue_at(&fixture.state, &guest, &request, "renew", now).expect("first grant");
    let id = upload_id(&guest, "renew");
    let expired = fixture
        .state
        .repository
        .upload_by_id(&id)
        .expect("reservation")
        .expires_at_ms;
    let _ = issue_at(&fixture.state, &guest, &request, "renew", expired).expect("renewed grant");
    assert!(
        fixture
            .state
            .repository
            .upload_by_id(&id)
            .expect("renewed reservation")
            .expires_at_ms
            > expired
    );

    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    let request = upload_request("renew-failure.txt", 1);
    let _ = issue_at(&fixture.state, &guest, &request, "renew-failure", now).expect("first grant");
    let id = upload_id(&guest, "renew-failure");
    let expired = fixture
        .state
        .repository
        .upload_by_id(&id)
        .expect("reservation")
        .expires_at_ms;
    assert_renewal_repository_failures(&fixture, &guest, &request, expired);
}
