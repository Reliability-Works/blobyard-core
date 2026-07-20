use super::*;
use crate::auth::Principal;
use crate::repository_fault_tests::FaultingRepository;
use crate::test_support::multipart_storage::MultipartStorage;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use blobyard_api_client::UploadStatusQuery;
use blobyard_contract::ReservationStrategy;

#[tokio::test]
async fn upload_request_propagates_multipart_provider_initialization_failure() {
    let (_root, mut state, _project) = fixture();
    state.storage = Arc::new(MultipartStorage::unavailable());
    let mut value = request("multipart-failure.bin");
    value.size_bytes = 100 * 1_024 * 1_024 + 1;
    assert_internal(
        send_json(
            &state,
            "POST",
            "/v1/uploads/request",
            serde_json::to_value(value).expect("request value"),
            Some("provider-failure"),
        )
        .await,
    )
    .await;
}

#[tokio::test]
async fn multipart_status_propagates_completed_part_listing_failure() {
    let (_root, mut state, project) = fixture();
    let mut value = request("multipart-status.bin");
    value.size_bytes = 5;
    let capability = SecretString::new("multipart-status-capability").expect("capability");
    let mut input = crate::transfer_grants::reservation_input(
        &value,
        &project,
        "upload_status_failure",
        &capability,
        i64::MAX as u64,
        blobyard_contract::ObjectSource::Cli,
    );
    input.strategy = ReservationStrategy::Multipart;
    input.part_size = Some(3);
    input.part_count = Some(2);
    state
        .repository
        .reserve_upload(&input)
        .expect("reservation");
    let principal = state
        .repository
        .authenticate_api_token(&hash("secret"), 2)
        .expect("principal");
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 2));

    let result = super::super::upload_status(
        State(state),
        crate::inbox_upload_auth::UploadAuthority::Operator(Principal(principal)),
        Query(UploadStatusQuery {
            upload_id: "upload_status_failure".to_owned(),
        }),
    )
    .await;
    let error = result.err().expect("expected multipart status failure");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
