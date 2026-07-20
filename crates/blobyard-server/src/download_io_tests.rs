#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{build_response, forward_chunk, next_chunk, parse_range, stream_reader};
use axum::{
    body::Bytes,
    http::{HeaderValue, StatusCode, header},
};
use blobyard_contract::{ByteRange, ObjectChecksum, StorageMetadata, StorageRead};
use http_body_util::BodyExt;
use std::io::{Cursor, Error, Read};

#[derive(Debug)]
enum FailingReader {
    BrokenPipe,
}

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::BrokenPipe => Err(Error::from(std::io::ErrorKind::BrokenPipe)),
        }
    }
}

fn failing_reader() -> FailingReader {
    FailingReader::BrokenPipe
}

#[test]
fn chunk_reader_and_forwarder_cover_data_eof_failure_and_closed_receivers() {
    let mut buffer = [0_u8; 8];
    assert_eq!(
        next_chunk(&mut Cursor::new(b"data"), &mut buffer)
            .expect("chunk")
            .expect("bytes"),
        Bytes::from_static(b"data")
    );
    assert!(
        next_chunk(&mut Cursor::new([]), &mut buffer)
            .expect("EOF")
            .is_none()
    );
    assert!(next_chunk(&mut failing_reader(), &mut buffer).is_err());

    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    assert!(forward_chunk(&sender, Ok(Some(Bytes::from_static(b"x")))));
    assert_eq!(
        receiver.blocking_recv().expect("forwarded").expect("bytes"),
        Bytes::from_static(b"x")
    );
    assert!(!forward_chunk(&sender, Ok(None)));
    assert!(!forward_chunk(&sender, Err(Error::other("read failure"))));
    assert!(receiver.blocking_recv().expect("forwarded error").is_err());
    drop(receiver);
    assert!(!forward_chunk(
        &sender,
        Ok(Some(Bytes::from_static(b"closed")))
    ));
}

#[test]
fn range_parser_accepts_supported_forms_and_rejects_every_invalid_shape() {
    assert_eq!(parse_range(None, 5).expect("full range"), None);
    for (value, size, expected) in [
        ("bytes=1-3", 5, (1, 4)),
        ("bytes=1-", 5, (1, 5)),
        ("bytes=-2", 5, (3, 5)),
        ("bytes=-9", 5, (0, 5)),
    ] {
        let value = HeaderValue::from_str(value).expect("range header");
        let range = parse_range(Some(&value), size)
            .expect("valid range")
            .expect("partial range");
        assert_eq!((range.start, range.end), expected);
    }

    let non_text = HeaderValue::from_bytes(&[0xff]).expect("opaque header");
    assert!(parse_range(Some(&non_text), 5).is_err());
    for (value, size) in [
        ("items=1-2", 5),
        ("bytes=1-2,3-4", 5),
        ("bytes=0-0", 0),
        ("bytes=1", 5),
        ("bytes=-x", 5),
        ("bytes=-0", 5),
        ("bytes=5-", 5),
        ("bytes=x-2", 5),
        ("bytes=1-x", 5),
        ("bytes=1-18446744073709551615", 5),
        ("bytes=4-2", 5),
    ] {
        let value = HeaderValue::from_str(value).expect("range header");
        assert!(parse_range(Some(&value), size).is_err(), "{value:?}");
    }
}

#[tokio::test]
async fn response_builder_and_reader_stream_preserve_bytes_headers_and_failures() {
    let read = StorageRead {
        reader: Box::new(Cursor::new(b"ell")),
        metadata: StorageMetadata {
            size: 5,
            checksum: ObjectChecksum::new("00".repeat(32)).expect("checksum"),
        },
        range: ByteRange::new(1, 4).expect("range"),
    };
    let response = build_response(read, StatusCode::PARTIAL_CONTENT).expect("response");
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 1-3/5");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "3");
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes(),
        Bytes::from_static(b"ell")
    );

    assert!(
        stream_reader(Box::new(failing_reader()))
            .collect()
            .await
            .is_err()
    );
}
