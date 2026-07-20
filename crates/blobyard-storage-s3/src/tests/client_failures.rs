use super::support::{ReplayEvent, SdkBody, StaticReplayClient, TestResult};
use crate::S3Credentials;
use crate::client::S3Client;
use crate::client_objects::{insert, metadata_headers};
use crate::transport::RequestBody;
use blobyard_contract::{MultipartPart, ObjectChecksum, StorageError};
use blobyard_core::SecretString;
use http::{HeaderMap, HeaderValue, Method, Request, Response, header};
use std::collections::HashMap;
use std::sync::Arc;

fn client(replay: &StaticReplayClient) -> Result<S3Client, Box<dyn std::error::Error>> {
    Ok(S3Client::new(
        Arc::new(replay.clone()),
        "http://localhost".parse()?,
        "us-east-1".to_owned(),
        "bucket".to_owned(),
        S3Credentials::new(
            SecretString::new("access")?,
            SecretString::new("secret")?,
            None,
        ),
        true,
    ))
}

fn event(
    method: Method,
    uri: &str,
    response: Response<SdkBody>,
) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(method)
            .uri(uri)
            .body(SdkBody::empty())?,
        response,
    ))
}

fn ok(body: SdkBody) -> Result<Response<SdkBody>, http::Error> {
    Response::builder().status(200).body(body)
}

fn checksum() -> Result<ObjectChecksum, StorageError> {
    ObjectChecksum::new("ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb")
}

#[tokio::test]
async fn client_and_inventory_propagate_transport_and_response_failures() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?
            .send(
                Method::GET,
                None,
                &[],
                HeaderMap::new(),
                RequestBody::Empty,
                S3Client::empty_hash(),
            )
            .await,
        Err(StorageError::Unavailable)
    ));

    let oversized = vec![b'x'; 4 * 1024 * 1024 + 1];
    let replay = StaticReplayClient::new(vec![event(
        Method::GET,
        "http://localhost/bucket?list-type=2",
        ok(SdkBody::from(oversized.as_slice()))?,
    )?]);
    assert!(matches!(
        client(&replay)?.list_objects(None, None).await,
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();
    Ok(())
}

#[tokio::test]
async fn multipart_creation_propagates_request_and_response_failures() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?
            .create_multipart("key", &HashMap::new())
            .await,
        Err(StorageError::Unavailable)
    ));

    let invalid_metadata = HashMap::from([("bad\nname".to_owned(), "value".to_owned())]);
    let replay = StaticReplayClient::new(Vec::new());
    assert_eq!(
        client(&replay)?
            .create_multipart("key", &invalid_metadata)
            .await,
        Err(StorageError::InvalidInput)
    );

    let oversized = vec![b'x'; 4 * 1024 * 1024 + 1];
    let replay = StaticReplayClient::new(vec![event(
        Method::POST,
        "http://localhost/bucket/key?uploads=",
        ok(SdkBody::from(oversized.as_slice()))?,
    )?]);
    assert!(matches!(
        client(&replay)?
            .create_multipart("key", &HashMap::new())
            .await,
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();
    Ok(())
}

#[tokio::test]
async fn multipart_parts_and_completion_fail_closed() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?
            .upload_part(
                "key",
                "upload",
                1,
                0,
                S3Client::empty_hash(),
                RequestBody::Empty
            )
            .await,
        Err(StorageError::Unavailable)
    ));

    let invalid_etag = HeaderValue::from_bytes(&[0xff])?;
    let response = Response::builder()
        .status(200)
        .header(header::ETAG, invalid_etag)
        .body(SdkBody::empty())?;
    let replay = StaticReplayClient::new(vec![event(
        Method::PUT,
        "http://localhost/bucket/key?partNumber=1&uploadId=upload",
        response,
    )?]);
    assert!(matches!(
        client(&replay)?
            .upload_part(
                "key",
                "upload",
                1,
                0,
                S3Client::empty_hash(),
                RequestBody::Empty
            )
            .await,
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();

    let missing_tag = MultipartPart {
        number: 1,
        size: 1,
        checksum: checksum()?,
        provider_tag: None,
    };
    let replay = StaticReplayClient::new(Vec::new());
    assert_eq!(
        client(&replay)?
            .complete_multipart("key", "upload", &[missing_tag])
            .await,
        Err(StorageError::InvalidInput)
    );
    Ok(())
}

#[tokio::test]
async fn oversized_error_bodies_fail_closed() -> TestResult {
    let oversized_error = vec![b'x'; 64 * 1024 + 1];
    let replay = StaticReplayClient::new(vec![event(
        Method::GET,
        "http://localhost/bucket",
        Response::builder()
            .status(500)
            .body(SdkBody::from(oversized_error.as_slice()))?,
    )?]);
    assert!(matches!(
        client(&replay)?
            .send(
                Method::GET,
                None,
                &[],
                HeaderMap::new(),
                RequestBody::Empty,
                S3Client::empty_hash(),
            )
            .await,
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();
    Ok(())
}

#[tokio::test]
async fn metadata_and_put_helpers_fail_closed() -> TestResult {
    let invalid_name = HashMap::from([("bad\nname".to_owned(), "value".to_owned())]);
    assert_eq!(
        metadata_headers(&invalid_name),
        Err(StorageError::InvalidInput)
    );
    let invalid_value = HashMap::from([("name".to_owned(), "bad\nvalue".to_owned())]);
    assert_eq!(
        metadata_headers(&invalid_value),
        Err(StorageError::InvalidInput)
    );
    let mut headers = HeaderMap::new();
    assert_eq!(
        insert(&mut headers, header::RANGE, "bad\nvalue"),
        Err(StorageError::InvalidInput)
    );
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?
            .put_object(
                "key",
                &HashMap::new(),
                0,
                S3Client::empty_hash(),
                RequestBody::Empty,
            )
            .await,
        Err(StorageError::Unavailable)
    ));
    let replay = StaticReplayClient::new(Vec::new());
    assert_eq!(
        client(&replay)?
            .put_object(
                "key",
                &invalid_name,
                0,
                S3Client::empty_hash(),
                RequestBody::Empty,
            )
            .await,
        Err(StorageError::InvalidInput)
    );
    Ok(())
}

#[tokio::test]
async fn head_failures_are_preserved() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?.head_object("key").await,
        Err(StorageError::Unavailable)
    ));

    let invalid_metadata = HeaderValue::from_bytes(&[0xff])?;
    let response = Response::builder()
        .status(200)
        .header("x-amz-meta-invalid", invalid_metadata)
        .body(SdkBody::empty())?;
    let replay = StaticReplayClient::new(vec![event(
        Method::HEAD,
        "http://localhost/bucket/key",
        response,
    )?]);
    assert!(matches!(
        client(&replay)?.head_object("key").await,
        Err(StorageError::IntegrityMismatch)
    ));
    replay.relaxed_requests_match();
    Ok(())
}

#[tokio::test]
async fn get_and_delete_failures_are_preserved() -> TestResult {
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?
            .get_object("key", None, tempfile::tempdir()?.path().join("download"))
            .await,
        Err(StorageError::Unavailable)
    ));
    let replay = StaticReplayClient::new(Vec::new());
    assert_eq!(
        client(&replay)?
            .get_object(
                "key",
                Some("bad\nrange"),
                tempfile::tempdir()?.path().join("download"),
            )
            .await,
        Err(StorageError::InvalidInput)
    );
    let replay = StaticReplayClient::new(Vec::new());
    assert!(matches!(
        client(&replay)?.delete_object("key").await,
        Err(StorageError::Unavailable)
    ));
    Ok(())
}
