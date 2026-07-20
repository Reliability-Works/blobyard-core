use super::support::{
    BUCKET, ReplayEvent, SdkBody, TestResult, error_event, event, head_event, object_uri, storage,
};
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
};
use http::{Method, Request, Response};

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const UPLOAD_ID: &str = "upload-123";

fn upload(key: &str) -> Result<MultipartId, StorageError> {
    crate::MultipartLocator::encode(&StorageKey::new(key)?, UPLOAD_ID)
}

fn part(tag: &str) -> Result<MultipartPart, StorageError> {
    Ok(MultipartPart {
        number: 1,
        size: 3,
        checksum: ObjectChecksum::new(ABC)?,
        provider_tag: Some(tag.to_owned()),
    })
}

fn complete_request_body() -> String {
    "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;tag-one&quot;</ETag></Part></CompleteMultipartUpload>".to_owned()
}

fn complete_event(key: &str) -> Result<ReplayEvent, http::Error> {
    let response_body = format!(
        "<CompleteMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>{BUCKET}</Bucket><Key>{key}</Key><ETag>\"complete\"</ETag></CompleteMultipartUploadResult>"
    );
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::POST)
            .uri(object_uri(key, Some("uploadId=upload-123")))
            .header("if-none-match", "*")
            .body(SdkBody::from(complete_request_body()))?,
        Response::builder()
            .status(200)
            .header("content-type", "application/xml")
            .body(SdkBody::from(response_body))?,
    ))
}

fn complete_error_event(key: &str, status: u16, code: &str) -> Result<ReplayEvent, http::Error> {
    let response_body = format!("<Error><Code>{code}</Code><Message>failure</Message></Error>");
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::POST)
            .uri(object_uri(key, Some("uploadId=upload-123")))
            .header("if-none-match", "*")
            .body(SdkBody::from(complete_request_body()))?,
        Response::builder()
            .status(status)
            .header("content-type", "application/xml")
            .body(SdkBody::from(response_body))?,
    ))
}

fn assert_completion_error(
    key: &str,
    events: Vec<ReplayEvent>,
    expected: StorageError,
) -> TestResult {
    let (_temporary, storage, replay) = storage(events, None)?;
    assert_eq!(
        storage.complete_multipart(&upload(key)?, &[part("\"tag-one\"")?]),
        Err(expected)
    );
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn completion_verifies_the_exact_persisted_object() -> TestResult {
    let key = "objects/completed.bin";
    let events = vec![
        complete_event(key)?,
        head_event(key, 3, ABC)?,
        head_event(key, 3, ABC)?,
        event(
            Method::GET,
            &object_uri(key, None),
            SdkBody::empty(),
            200,
            SdkBody::from("abc"),
        )?,
    ];
    let (_temporary, storage, replay) = storage(events, None)?;
    let result = storage.complete_multipart(&upload(key)?, &[part("\"tag-one\"")?]);
    assert!(
        result.is_ok(),
        "completion failed after {} provider requests: {result:?}",
        replay.actual_requests().count()
    );
    let metadata = result?;
    assert_eq!(metadata.size, 3);
    assert_eq!(metadata.checksum, ObjectChecksum::new(ABC)?);
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn completion_provider_failure_is_mapped_without_verification() -> TestResult {
    let key = "objects/conflict.bin";
    let (_temporary, storage, replay) = storage(
        vec![complete_error_event(key, 412, "PreconditionFailed")?],
        None,
    )?;
    assert_eq!(
        storage.complete_multipart(&upload(key)?, &[part("\"tag-one\"")?]),
        Err(StorageError::Conflict)
    );
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn failed_completion_verification_removes_the_provider_object() -> TestResult {
    let key = "objects/corrupt.bin";
    let events = vec![
        complete_event(key)?,
        error_event(Method::HEAD, &object_uri(key, None), 404, "NoSuchKey")?,
        event(
            Method::DELETE,
            &object_uri(key, None),
            SdkBody::empty(),
            204,
            SdkBody::empty(),
        )?,
    ];
    assert_completion_error(key, events, StorageError::NotFound)
}

#[test]
fn corrupt_completed_bytes_are_removed_before_returning() -> TestResult {
    let key = "objects/corrupt-bytes.bin";
    let events = vec![
        complete_event(key)?,
        head_event(key, 3, ABC)?,
        head_event(key, 3, ABC)?,
        event(
            Method::GET,
            &object_uri(key, None),
            SdkBody::empty(),
            200,
            SdkBody::from("ab"),
        )?,
        event(
            Method::DELETE,
            &object_uri(key, None),
            SdkBody::empty(),
            204,
            SdkBody::empty(),
        )?,
    ];
    assert_completion_error(key, events, StorageError::IntegrityMismatch)
}

#[test]
fn missing_object_during_cleanup_preserves_the_verification_error() -> TestResult {
    let key = "objects/disappeared.bin";
    let events = vec![
        complete_event(key)?,
        error_event(Method::HEAD, &object_uri(key, None), 404, "NoSuchKey")?,
        error_event(Method::DELETE, &object_uri(key, None), 404, "NoSuchKey")?,
    ];
    assert_completion_error(key, events, StorageError::NotFound)
}

#[test]
fn failed_verification_reports_cleanup_failure() -> TestResult {
    let key = "objects/cleanup-failure.bin";
    let events = vec![
        complete_event(key)?,
        error_event(Method::HEAD, &object_uri(key, None), 400, "InvalidArgument")?,
        error_event(
            Method::DELETE,
            &object_uri(key, None),
            400,
            "InvalidArgument",
        )?,
    ];
    assert_completion_error(key, events, StorageError::Unavailable)
}
