use super::support::{ReplayEvent, SdkBody, TestResult, event, head_event, object_uri, storage};
use blobyard_contract::{ByteRange, ObjectStorage, StorageKey};
use http::{Method, Request, Response};
use std::io::{Cursor, Read};

const CHECKSUM: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

fn put_event(key: &str) -> Result<ReplayEvent, http::Error> {
    Ok(ReplayEvent::new(
        Request::builder()
            .method(Method::PUT)
            .uri(object_uri(key, None))
            .header("if-none-match", "*")
            .header("x-amz-meta-blobyard-size", "5")
            .header("x-amz-meta-blobyard-sha256", CHECKSUM)
            .body(SdkBody::empty())?,
        Response::builder().status(200).body(SdkBody::empty())?,
    ))
}

#[test]
fn object_round_trip_uses_conditional_put_metadata_ranges_and_explicit_delete() -> TestResult {
    let key = "tenant/objects/hello.txt";
    let events = vec![
        put_event(key)?,
        head_event(key, 5, CHECKSUM)?,
        head_event(key, 5, CHECKSUM)?,
        event(
            Method::GET,
            &object_uri(key, None),
            SdkBody::empty(),
            200,
            SdkBody::from("hello"),
        )?,
        head_event(key, 5, CHECKSUM)?,
        ReplayEvent::new(
            Request::builder()
                .method(Method::GET)
                .uri(object_uri(key, None))
                .header("range", "bytes=1-3")
                .body(SdkBody::empty())?,
            Response::builder().status(206).body(SdkBody::from("ell"))?,
        ),
        head_event(key, 5, CHECKSUM)?,
        event(
            Method::DELETE,
            &object_uri(key, None),
            SdkBody::empty(),
            204,
            SdkBody::empty(),
        )?,
    ];
    let (_temporary, storage, replay) = storage(events, Some("tenant"))?;
    let object_key = StorageKey::new("objects/hello.txt")?;
    let metadata = storage.put(&object_key, &mut Cursor::new(b"hello"), None)?;
    assert_eq!(metadata.size, 5);
    assert_eq!(
        replay
            .actual_requests()
            .next()
            .and_then(|request| {
                request
                    .headers()
                    .get("content-length")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            })
            .as_deref(),
        Some("5")
    );
    assert_eq!(storage.head(&object_key)?, metadata);
    let mut full = storage.get(&object_key, None)?;
    let mut full_bytes = Vec::new();
    full.reader.read_to_end(&mut full_bytes)?;
    assert_eq!(full_bytes, b"hello");
    let mut partial = storage.get(&object_key, Some(ByteRange::new(1, 4)?))?;
    let mut partial_bytes = Vec::new();
    partial.reader.read_to_end(&mut partial_bytes)?;
    assert_eq!(partial_bytes, b"ell");
    storage.delete(&object_key)?;
    replay.relaxed_requests_match();
    Ok(())
}
