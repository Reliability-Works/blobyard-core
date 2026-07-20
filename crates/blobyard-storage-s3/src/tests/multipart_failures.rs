use super::support::{
    ABC, BUCKET, ReplayEvent, SdkBody, TestResult, error_event, event, expected_abc, object_uri,
    storage, storage_with_body_builder, unavailable_body_builder,
};
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
};
use http::{Method, Request, Response};
use std::io::Cursor;

fn create_response_event(key: &str, upload_id: Option<&str>) -> Result<ReplayEvent, http::Error> {
    let upload_id =
        upload_id.map_or_else(String::new, |value| format!("<UploadId>{value}</UploadId>"));
    let body = format!(
        "<InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>{BUCKET}</Bucket><Key>{key}</Key>{upload_id}</InitiateMultipartUploadResult>"
    );
    create_replay_event(key, 200, body)
}

fn create_error_event(key: &str, code: &str) -> Result<ReplayEvent, http::Error> {
    create_replay_event(
        key,
        400,
        format!("<Error><Code>{code}</Code><Message>failure</Message></Error>"),
    )
}

fn create_replay_event(key: &str, status: u16, body: String) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::POST)
            .uri(object_uri(key, Some("uploads=")))
            .header("x-amz-meta-blobyard-size", "3")
            .header("x-amz-meta-blobyard-sha256", ABC)
            .body(SdkBody::empty())?,
        Response::builder()
            .status(status)
            .header("content-type", "application/xml")
            .body(SdkBody::from(body))?,
    ))
}

fn part_response_event(key: &str, tag: Option<&str>) -> Result<ReplayEvent, http::Error> {
    let mut response = Response::builder().status(200);
    if let Some(tag) = tag {
        response = response.header("etag", tag);
    }
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::PUT)
            .uri(object_uri(key, Some("partNumber=1&uploadId=upload-123")))
            .body(SdkBody::empty())?,
        response.body(SdkBody::empty())?,
    ))
}

#[test]
fn begin_provider_failures_are_stable() -> TestResult {
    let key = "objects/provider-failure.bin";
    let object_key = StorageKey::new(key)?;
    let (_temporary, failed_head, replay) = storage(
        vec![error_event(
            Method::HEAD,
            &object_uri(key, None),
            400,
            "InvalidArgument",
        )?],
        None,
    )?;
    assert_eq!(
        failed_head.begin_multipart(&object_key, &expected_abc()?),
        Err(StorageError::InvalidInput)
    );
    replay.relaxed_requests_match();

    for event in [
        create_response_event(key, None)?,
        create_error_event(key, "InvalidRequest")?,
    ] {
        let events = vec![
            error_event(Method::HEAD, &object_uri(key, None), 404, "NoSuchKey")?,
            event,
        ];
        let (_temporary, adapter, replay) = storage(events, None)?;
        assert!(
            adapter
                .begin_multipart(&object_key, &expected_abc()?)
                .is_err()
        );
        replay.relaxed_requests_match();
    }
    Ok(())
}

#[test]
fn part_provider_and_staging_failures_are_stable() -> TestResult {
    let key = "objects/provider-failure.bin";
    let object_key = StorageKey::new(key)?;
    let upload = crate::MultipartLocator::encode(&object_key, "upload-123")?;
    let oversized_tag = "x".repeat(513);
    for tag in [None, Some(oversized_tag.as_str())] {
        let (_temporary, invalid_tag, replay) =
            storage(vec![part_response_event(key, tag)?], None)?;
        assert_eq!(
            invalid_tag.put_part(&upload, 1, &mut Cursor::new(b"abc")),
            Err(StorageError::Unavailable)
        );
        replay.relaxed_requests_match();
    }

    let uri = object_uri(key, Some("partNumber=1&uploadId=upload-123"));
    let (_temporary, provider_failure, replay) = storage(
        vec![error_event(Method::PUT, &uri, 400, "InvalidPart")?],
        None,
    )?;
    assert_eq!(
        provider_failure.put_part(&upload, 1, &mut Cursor::new(b"abc")),
        Err(StorageError::InvalidInput)
    );
    replay.relaxed_requests_match();

    let (temporary, missing_stage, replay) = storage(Vec::new(), None)?;
    temporary.close()?;
    assert_eq!(
        missing_stage.put_part(&upload, 1, &mut Cursor::new(b"abc")),
        Err(StorageError::Unavailable)
    );
    replay.relaxed_requests_match();

    let (_temporary, unavailable_body, replay) =
        storage_with_body_builder(Vec::new(), None, unavailable_body_builder)?;
    assert_eq!(
        unavailable_body.put_part(&upload, 1, &mut Cursor::new(b"abc")),
        Err(StorageError::Unavailable)
    );
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn malformed_locators_fail_closed() -> TestResult {
    let malformed = MultipartId("not-a-provider-locator".to_owned());
    let (_temporary, storage_adapter, replay) = storage(Vec::new(), None)?;
    assert_eq!(
        storage_adapter.abort_multipart(&malformed),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        storage_adapter.put_part(&malformed, 1, &mut Cursor::new(b"abc")),
        Err(StorageError::InvalidInput)
    );
    let valid_part = MultipartPart {
        number: 1,
        size: 3,
        checksum: ObjectChecksum::new(ABC)?,
        provider_tag: Some("tag".to_owned()),
    };
    assert_eq!(
        storage_adapter.complete_multipart(&malformed, &[valid_part]),
        Err(StorageError::InvalidInput)
    );
    replay.relaxed_requests_match();

    Ok(())
}

#[test]
fn provider_abort_errors_fail_closed() -> TestResult {
    let key = StorageKey::new("objects/aborted.bin")?;
    let upload = crate::MultipartLocator::encode(&key, "upload-123")?;
    let uri = object_uri(key.as_str(), Some("max-parts=1&uploadId=upload-123"));
    let (_temporary, missing, replay) = storage(
        vec![error_event(Method::GET, &uri, 404, "NoSuchUpload")?],
        None,
    )?;
    assert_eq!(
        missing.abort_multipart(&upload),
        Err(StorageError::NotFound)
    );
    replay.relaxed_requests_match();

    let list_uri = object_uri(key.as_str(), Some("max-parts=1&uploadId=upload-123"));
    let abort_uri = object_uri(key.as_str(), Some("uploadId=upload-123"));
    let list_body = format!(
        "<ListPartsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>upload-123</UploadId><MaxParts>1</MaxParts><IsTruncated>false</IsTruncated></ListPartsResult>",
        key = key.as_str(),
    );
    let events = vec![
        event(
            Method::GET,
            &list_uri,
            SdkBody::empty(),
            200,
            SdkBody::from(list_body),
        )?,
        error_event(Method::DELETE, &abort_uri, 503, "SlowDown")?,
    ];
    let (_temporary, unavailable, replay) = storage(events, None)?;
    assert_eq!(
        unavailable.abort_multipart(&upload),
        Err(StorageError::Unavailable)
    );
    replay.relaxed_requests_match();
    Ok(())
}
