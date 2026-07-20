use crate::S3Storage;
use crate::client::S3Client;
pub(super) use crate::replay::{ReplayEvent, StaticReplayClient, TestBody as SdkBody};
use blobyard_contract::{ObjectChecksum, StorageError, StorageMetadata};
use blobyard_core::SecretString;
use http::{Method, Request, Response};
use std::error::Error;

pub(super) const BUCKET: &str = "test-bucket";
pub(super) const ENDPOINT: &str = "http://localhost";
pub(super) const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
pub(super) type TestResult = Result<(), Box<dyn Error>>;

pub(super) fn expected_abc() -> Result<StorageMetadata, StorageError> {
    Ok(StorageMetadata {
        size: 3,
        checksum: ObjectChecksum::new(ABC)?,
    })
}

pub(super) fn storage(
    events: Vec<ReplayEvent>,
    prefix: Option<&str>,
) -> Result<(tempfile::TempDir, S3Storage, StaticReplayClient), Box<dyn Error>> {
    storage_with_body_builder(events, prefix, crate::S3Storage::byte_stream)
}

pub(super) fn storage_with_body_builder(
    events: Vec<ReplayEvent>,
    prefix: Option<&str>,
    body_builder: crate::BodyBuilder,
) -> Result<(tempfile::TempDir, S3Storage, StaticReplayClient), Box<dyn Error>> {
    let replay = StaticReplayClient::new(events);
    let client = test_client(&replay)?;
    let temporary = tempfile::tempdir()?;
    let storage = S3Storage::from_test_client_with_body_builder(
        client,
        BUCKET,
        prefix,
        temporary.path().to_path_buf(),
        body_builder,
    )?;
    Ok((temporary, storage, replay))
}

fn test_client(replay: &StaticReplayClient) -> Result<S3Client, Box<dyn Error>> {
    Ok(S3Client::new(
        std::sync::Arc::new(replay.clone()),
        ENDPOINT.parse()?,
        "us-east-1".to_owned(),
        BUCKET.to_owned(),
        crate::S3Credentials::new(
            SecretString::new("test-access")?,
            SecretString::new("test-secret")?,
            None,
        ),
        true,
    ))
}

pub(super) fn unavailable_body_builder(_path: std::path::PathBuf) -> crate::BodyFuture {
    Box::pin(async { Err(blobyard_contract::StorageError::Unavailable) })
}

#[test]
fn adapter_construction_failures_are_stable() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    let client = test_client(&replay)?;
    let temporary = tempfile::tempdir()?;
    let invalid_directory = temporary.path().join("file");
    std::fs::write(&invalid_directory, b"file")?;
    assert_eq!(
        S3Storage::from_test_client_with_body_builder(
            client.clone(),
            BUCKET,
            None,
            invalid_directory,
            crate::S3Storage::byte_stream,
        )
        .err(),
        Some(blobyard_contract::StorageError::Unavailable)
    );
    assert_eq!(
        S3Storage::from_test_parts(
            client,
            BUCKET,
            None,
            temporary.path().to_path_buf(),
            crate::S3Storage::byte_stream,
            Err(blobyard_contract::StorageError::Unavailable),
        )
        .err(),
        Some(blobyard_contract::StorageError::Unavailable)
    );
    Ok(())
}

pub(super) fn event(
    method: Method,
    uri: &str,
    request_body: impl Into<SdkBody>,
    status: u16,
    response_body: impl Into<SdkBody>,
) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(method)
            .uri(uri)
            .body(request_body.into())?,
        Response::builder()
            .status(status)
            .body(response_body.into())?,
    ))
}

pub(super) fn object_uri(key: &str, query: Option<&str>) -> String {
    let object = if key.is_empty() {
        format!("{ENDPOINT}/{BUCKET}")
    } else {
        format!("{ENDPOINT}/{BUCKET}/{key}")
    };
    query.map_or_else(|| object.clone(), |value| format!("{object}?{value}"))
}

pub(super) fn head_event(key: &str, size: u64, checksum: &str) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::HEAD)
            .uri(object_uri(key, None))
            .body(SdkBody::empty())?,
        Response::builder()
            .status(200)
            .header("content-length", size.to_string())
            .header("x-amz-meta-blobyard-size", size.to_string())
            .header("x-amz-meta-blobyard-sha256", checksum)
            .body(SdkBody::empty())?,
    ))
}

pub(super) fn error_event(
    method: Method,
    uri: &str,
    status: u16,
    code: &str,
) -> Result<ReplayEvent, http::Error> {
    let body = format!("<Error><Code>{code}</Code><Message>failure</Message></Error>");
    Ok(ReplayEvent::new(
        Request::builder()
            .method(method)
            .uri(uri)
            .body(SdkBody::empty())?,
        Response::builder()
            .status(status)
            .header("content-type", "application/xml")
            .body(SdkBody::from(body))?,
    ))
}
