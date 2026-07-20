use super::support::{
    ABC, BUCKET, ReplayEvent, SdkBody, TestResult, error_event, event, expected_abc, head_event,
    object_uri, storage,
};
use blobyard_contract::{MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey};
use http::{Method, Request, Response};
use std::io::Cursor;

fn create_event(key: &str) -> Result<ReplayEvent, http::Error> {
    let body = format!(
        "<InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>upload-123</UploadId></InitiateMultipartUploadResult>"
    );
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::POST)
            .uri(object_uri(key, Some("uploads=")))
            .header("x-amz-meta-blobyard-size", "3")
            .header("x-amz-meta-blobyard-sha256", ABC)
            .body(SdkBody::empty())?,
        Response::builder()
            .status(200)
            .header("content-type", "application/xml")
            .body(SdkBody::from(body))?,
    ))
}

fn part_event(key: &str) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::PUT)
            .uri(object_uri(key, Some("partNumber=1&uploadId=upload-123")))
            .body(SdkBody::empty())?,
        Response::builder()
            .status(200)
            .header("etag", "\"tag-one\"")
            .body(SdkBody::empty())?,
    ))
}

fn list_parts_event(key: &str) -> Result<ReplayEvent, http::Error> {
    let body = format!(
        "<ListPartsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>upload-123</UploadId><MaxParts>1</MaxParts><IsTruncated>false</IsTruncated></ListPartsResult>"
    );
    event(
        Method::GET,
        &object_uri(key, Some("max-parts=1&uploadId=upload-123")),
        SdkBody::empty(),
        200,
        SdkBody::from(body),
    )
}

#[test]
fn native_multipart_begin_part_and_abort_preserve_provider_tag() -> TestResult {
    let key = "objects/multipart.bin";
    let events = vec![
        error_event(Method::HEAD, &object_uri(key, None), 404, "NoSuchKey")?,
        create_event(key)?,
        part_event(key)?,
        list_parts_event(key)?,
        event(
            Method::DELETE,
            &object_uri(key, Some("uploadId=upload-123")),
            SdkBody::empty(),
            204,
            SdkBody::empty(),
        )?,
    ];
    let (_temporary, storage, replay) = storage(events, None)?;
    let object_key = StorageKey::new(key)?;
    let upload = storage.begin_multipart(&object_key, &expected_abc()?)?;
    assert!(!upload.0.contains("upload-123"));
    let part = storage.put_part(&upload, 1, &mut Cursor::new(b"abc"))?;
    assert_eq!(part.number, 1);
    assert_eq!(part.size, 3);
    assert_eq!(part.provider_tag.as_deref(), Some("\"tag-one\""));
    storage.abort_multipart(&upload)?;
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn multipart_part_numbers_fail_before_provider_access() -> TestResult {
    let (_temporary, storage, replay) = storage(Vec::new(), None)?;
    let upload =
        crate::MultipartLocator::encode(&StorageKey::new("objects/multipart.bin")?, "upload-123")?;
    for number in [0, 10_001, u32::MAX] {
        assert_eq!(
            storage.put_part(&upload, number, &mut Cursor::new(b"abc")),
            Err(StorageError::InvalidInput)
        );
    }
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn multipart_completion_validation_fails_before_provider_access() -> TestResult {
    let (_temporary, storage, replay) = storage(Vec::new(), None)?;
    let upload =
        crate::MultipartLocator::encode(&StorageKey::new("objects/multipart.bin")?, "upload-123")?;
    let checksum = ObjectChecksum::new(ABC)?;
    let valid = MultipartPart {
        number: 1,
        size: 3,
        checksum: checksum.clone(),
        provider_tag: Some("tag".to_owned()),
    };
    let invalid_sets = [
        Vec::new(),
        vec![MultipartPart {
            number: 2,
            ..valid.clone()
        }],
        vec![MultipartPart {
            provider_tag: None,
            ..valid.clone()
        }],
        vec![MultipartPart {
            provider_tag: Some("bad\ntag".to_owned()),
            ..valid
        }],
        vec![MultipartPart {
            number: 1,
            size: 3,
            checksum: checksum.clone(),
            provider_tag: Some(String::new()),
        }],
        vec![MultipartPart {
            number: 1,
            size: 3,
            checksum,
            provider_tag: Some("x".repeat(513)),
        }],
    ];
    for parts in invalid_sets {
        assert_eq!(
            storage.complete_multipart(&upload, &parts),
            Err(StorageError::InvalidInput)
        );
    }
    let oversized = vec![
        MultipartPart {
            number: 1,
            size: 3,
            checksum: ObjectChecksum::new(ABC)?,
            provider_tag: Some("tag".to_owned()),
        };
        10_001
    ];
    assert_eq!(
        storage.complete_multipart(&upload, &oversized),
        Err(StorageError::InvalidInput)
    );
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn begin_rejects_existing_object_without_creating_provider_state() -> TestResult {
    let key = "objects/existing.bin";
    let (_temporary, storage, replay) = storage(vec![head_event(key, 3, ABC)?], None)?;
    assert_eq!(
        storage.begin_multipart(&StorageKey::new(key)?, &expected_abc()?),
        Err(StorageError::Conflict)
    );
    replay.relaxed_requests_match();
    Ok(())
}
