use super::support::{
    ReplayEvent, SdkBody, TestResult, error_event, event, head_event, object_uri, storage,
    storage_with_body_builder, unavailable_body_builder,
};
use blobyard_contract::{ByteRange, ObjectChecksum, ObjectStorage, StorageError, StorageKey};
use http::{Method, Request, Response};
use std::io::{Cursor, Read};

const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
const OTHER: &str = "3733cd977ff8eb18b987357e22ced99f46097f31ecb239e878ae63760e83e4d5";

#[test]
fn missing_conflict_and_provider_errors_map_to_stable_classes() -> TestResult {
    let key = StorageKey::new("missing")?;
    let uri = object_uri("missing", None);
    let (_temporary, missing, replay) = storage(
        vec![error_event(Method::HEAD, &uri, 404, "NoSuchKey")?],
        None,
    )?;
    assert_eq!(missing.head(&key), Err(StorageError::NotFound));
    replay.relaxed_requests_match();

    let put_uri = object_uri("conflict", None);
    let (_temporary, conflict, replay) = storage(
        vec![error_event(
            Method::PUT,
            &put_uri,
            412,
            "PreconditionFailed",
        )?],
        None,
    )?;
    assert_eq!(
        conflict.put(
            &StorageKey::new("conflict")?,
            &mut Cursor::new(b"hello"),
            None,
        ),
        Err(StorageError::Conflict)
    );
    replay.relaxed_requests_match();

    let (_temporary, unavailable, replay) = storage(
        vec![error_event(Method::HEAD, &uri, 503, "SlowDown")?],
        None,
    )?;
    assert_eq!(unavailable.head(&key), Err(StorageError::Unavailable));
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn integrity_and_range_failures_stop_before_unsafe_follow_up_requests() -> TestResult {
    let key = StorageKey::new("object")?;
    let (_temporary, checksum_storage, replay) = storage(Vec::new(), None)?;
    assert_eq!(
        checksum_storage.put(
            &key,
            &mut Cursor::new(b"hello"),
            Some(&ObjectChecksum::new(OTHER)?),
        ),
        Err(StorageError::IntegrityMismatch)
    );
    replay.relaxed_requests_match();

    let (temporary, staging_failure, replay) = storage(Vec::new(), None)?;
    temporary.close()?;
    assert_eq!(
        staging_failure.put(&key, &mut Cursor::new(b"hello"), None),
        Err(StorageError::Unavailable)
    );
    replay.relaxed_requests_match();

    let (_temporary, unavailable_body, replay) =
        storage_with_body_builder(Vec::new(), None, unavailable_body_builder)?;
    assert_eq!(
        unavailable_body.put(&key, &mut Cursor::new(b"hello"), None),
        Err(StorageError::Unavailable)
    );
    replay.relaxed_requests_match();

    let (_temporary, invalid_range, replay) = storage(vec![head_event("object", 5, HELLO)?], None)?;
    assert!(matches!(
        invalid_range.get(&key, Some(ByteRange::new(4, 6)?)),
        Err(StorageError::InvalidInput)
    ));
    replay.relaxed_requests_match();

    let (_temporary, empty_range, replay) = storage(vec![head_event("object", 5, HELLO)?], None)?;
    let mut empty = empty_range.get(&key, Some(ByteRange::new(3, 3)?))?;
    let mut bytes = Vec::new();
    empty.reader.read_to_end(&mut bytes)?;
    assert!(bytes.is_empty());
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn download_setup_and_provider_failures_are_stable() -> TestResult {
    let key = StorageKey::new("object")?;
    let head_uri = object_uri("object", None);
    let (_temporary, missing, replay) = storage(
        vec![error_event(Method::HEAD, &head_uri, 404, "NoSuchKey")?],
        None,
    )?;
    assert!(matches!(
        missing.get(&key, None),
        Err(StorageError::NotFound)
    ));
    replay.relaxed_requests_match();

    let (temporary, no_staging_directory, replay) =
        storage(vec![head_event("object", 5, HELLO)?], None)?;
    temporary.close()?;
    assert!(matches!(
        no_staging_directory.get(&key, None),
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();

    let events = vec![
        head_event("object", 5, HELLO)?,
        error_event(Method::GET, &object_uri("object", None), 503, "SlowDown")?,
    ];
    let (_temporary, unavailable, replay) = storage(events, None)?;
    assert!(matches!(
        unavailable.get(&key, None),
        Err(StorageError::Unavailable)
    ));
    replay.relaxed_requests_match();
    Ok(())
}

#[test]
fn corrupt_metadata_and_bytes_fail_integrity_checks() -> TestResult {
    let key = StorageKey::new("object")?;
    let corrupt_head = ReplayEvent::new(
        Request::builder()
            .method(Method::HEAD)
            .uri(object_uri("object", None))
            .body(SdkBody::empty())?,
        Response::builder()
            .status(200)
            .header("content-length", "5")
            .header("x-amz-meta-blobyard-size", "4")
            .header("x-amz-meta-blobyard-sha256", HELLO)
            .body(SdkBody::empty())?,
    );
    let (_temporary, storage_adapter, replay) = storage(vec![corrupt_head], None)?;
    assert_eq!(
        storage_adapter.head(&key),
        Err(StorageError::IntegrityMismatch)
    );
    replay.relaxed_requests_match();

    for body in ["four", "HELLO"] {
        let events = vec![
            head_event("object", 5, HELLO)?,
            event(
                Method::GET,
                &object_uri("object", None),
                SdkBody::empty(),
                200,
                SdkBody::from(body),
            )?,
        ];
        let (_temporary, storage_adapter, replay) = storage(events, None)?;
        assert!(matches!(
            storage_adapter.get(&key, None),
            Err(StorageError::IntegrityMismatch)
        ));
        replay.relaxed_requests_match();
    }
    Ok(())
}

#[test]
fn delete_requires_existing_metadata_and_forwards_provider_failure() -> TestResult {
    let key = StorageKey::new("object")?;
    let uri = object_uri("object", None);
    let (_temporary, missing, replay) = storage(
        vec![error_event(Method::HEAD, &uri, 404, "NoSuchKey")?],
        None,
    )?;
    assert_eq!(missing.delete(&key), Err(StorageError::NotFound));
    replay.relaxed_requests_match();

    let events = vec![
        head_event("object", 5, HELLO)?,
        error_event(Method::DELETE, &object_uri("object", None), 503, "SlowDown")?,
    ];
    let (_temporary, unavailable, replay) = storage(events, None)?;
    assert_eq!(unavailable.delete(&key), Err(StorageError::Unavailable));
    replay.relaxed_requests_match();
    Ok(())
}
