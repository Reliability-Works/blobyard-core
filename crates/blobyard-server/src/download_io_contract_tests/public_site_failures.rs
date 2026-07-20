use super::*;

#[tokio::test]
async fn public_site_adapter_rejects_corrupt_content_metadata() {
    let mut invalid_content_type = stored_object("valid/key");
    invalid_content_type.content_type = "text/html\nunsafe".to_owned();
    assert_internal(public_response(&invalid_content_type, &Method::GET).await).await;

    let mut missing_checksum = stored_object("valid/key");
    missing_checksum.version.checksum = None;
    assert_internal(public_response(&missing_checksum, &Method::GET).await).await;

    let mut invalid_checksum = stored_object("valid/key");
    invalid_checksum.version.checksum = Some("invalid\nchecksum".to_owned());
    assert_internal(public_response(&invalid_checksum, &Method::GET).await).await;

    assert_internal(
        public_response_with(
            StorageBehavior::HeadError(StorageError::Unavailable),
            &stored_object("valid/key"),
            &Method::GET,
        )
        .await,
    )
    .await;
}

async fn assert_internal(response: TestResponse) {
    assert_error(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        "Blobyard couldn't complete that. Try again or contact support.",
    )
    .await;
}
