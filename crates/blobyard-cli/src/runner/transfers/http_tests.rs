#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::http::{
    SignedTransferClient, chunk_capacity, file_body, header_value, retry_delay, retryable_status,
    signed_headers_with, valid_etag,
};
use blobyard_api_client::SignedHeader;
use blobyard_core::{ErrorCode, SecretString};
use http_body_util::BodyExt;
use tokio::net::TcpListener;

struct Reply {
    status: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: Vec<u8>,
}

async fn serve(replies: Vec<Reply>) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let request = crate::request_capture::capture(&mut socket, "read").await;
            crate::request_capture::write_response(
                &mut socket,
                reply.status,
                &reply.headers,
                &reply.body,
            )
            .await;
            requests.push(request);
        }
        requests
    });
    (format!("http://{address}/object"), task)
}

fn reply(status: &'static str) -> Reply {
    Reply {
        status,
        headers: Vec::new(),
        body: Vec::new(),
    }
}

fn progress() -> indicatif::ProgressBar {
    indicatif::ProgressBar::hidden()
}

#[tokio::test]
async fn upload_body_stream_finishes_after_the_declared_range() {
    let temp = tempfile::tempdir().expect("temp");
    let source = temp.path().join("source.bin");
    std::fs::write(&source, b"abcdef").expect("source");
    let body = file_body(&source, 1, 3, None).await.expect("body");
    let collected = body.collect().await.expect("collect").to_bytes();
    assert_eq!(collected.as_ref(), b"bcd");
}

#[tokio::test]
async fn signed_transfer_streams_put_parts_and_downloads() {
    let temp = tempfile::tempdir().expect("temp");
    let source = temp.path().join("source.bin");
    std::fs::write(&source, b"abcdef").expect("source");
    let replies = vec![
        reply("200 OK"),
        reply("503 Unavailable"),
        Reply {
            status: "200 OK",
            headers: vec![("ETag", "etag-2")],
            body: Vec::new(),
        },
        Reply {
            status: "200 OK",
            headers: Vec::new(),
            body: b"abcdef".to_vec(),
        },
    ];
    let (url, task) = serve(replies).await;
    let secret = SecretString::new(url).expect("url");
    let headers = [SignedHeader {
        name: "content-type".into(),
        value: SecretString::new("application/octet-stream").expect("header"),
    }];
    let client = SignedTransferClient::new();
    let progress = progress();
    client
        .put_file(&secret, &source, 6, &headers, &progress)
        .await
        .expect("single put");
    assert_eq!(
        client
            .put_part(&secret, &source, 2, 3, &progress)
            .await
            .expect("part"),
        "etag-2"
    );
    let output = temp.path().join("download.bin");
    let measured = client
        .download(&secret, &output, &progress)
        .await
        .expect("download");
    assert_eq!(measured.size_bytes, 6);
    assert_eq!(std::fs::read(output).expect("output"), b"abcdef");
    let requests = task.await.expect("server");
    assert!(requests[0].windows(6).any(|window| window == b"abcdef"));
    assert!(requests[2].windows(3).any(|window| window == b"cde"));
}

#[tokio::test]
async fn signed_transfer_maps_http_failures() {
    let temp = tempfile::tempdir().expect("temp");
    let source = temp.path().join("source.bin");
    std::fs::write(&source, b"abc").expect("source");
    let (url, task) = serve(vec![
        reply("400 Bad Request"),
        reply("200 OK"),
        reply("404 Not Found"),
    ])
    .await;
    let secret = SecretString::new(url).expect("url");
    let client = SignedTransferClient::new();
    let progress = progress();
    assert_eq!(
        client
            .put_file(&secret, &source, 3, &[], &progress)
            .await
            .expect_err("put")
            .code(),
        ErrorCode::StorageError
    );
    assert_eq!(
        client
            .put_part(&secret, &source, 0, 3, &progress)
            .await
            .expect_err("etag")
            .code(),
        ErrorCode::StorageError
    );
    assert_eq!(
        client
            .download(&secret, &temp.path().join("missing"), &progress)
            .await
            .expect_err("get")
            .code(),
        ErrorCode::StorageError
    );
    task.await.expect("server");
}

#[tokio::test]
async fn signed_transfer_maps_network_failure() {
    let temp = tempfile::tempdir().expect("temp");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let dead_url = format!("http://{}/dead", listener.local_addr().expect("address"));
    drop(listener);
    let dead = SecretString::new(dead_url).expect("dead url");
    let error = SignedTransferClient::new()
        .download(&dead, &temp.path().join("network"), &progress())
        .await
        .expect_err("network");
    assert_eq!(error.code(), ErrorCode::NetworkError);

    let source = temp.path().join("source");
    std::fs::write(&source, b"abc").expect("source");
    let part_error = SignedTransferClient::new()
        .put_part(&dead, &source, 0, 3, &progress())
        .await
        .expect_err("part network");
    assert_eq!(part_error.code(), ErrorCode::NetworkError);

    let missing = temp.path().join("missing");
    assert!(
        SignedTransferClient::new()
            .put_part(&dead, &missing, 0, 3, &progress())
            .await
            .is_err()
    );
}

#[test]
fn signed_transfer_validators_cover_bounds() {
    assert!(!valid_etag(""));
    assert!(!valid_etag(&"x".repeat(1_025)));
    assert!(!valid_etag("line\nbreak"));
    assert!(valid_etag("etag"));
    for status in [408, 425, 429, 500, 502, 503, 504] {
        assert!(retryable_status(
            reqwest::StatusCode::from_u16(status).expect("status")
        ));
    }
    assert!(!retryable_status(reqwest::StatusCode::BAD_REQUEST));
    let delay = retry_delay(1);
    assert!((200..=300).contains(&delay.as_millis()));
    assert_eq!(chunk_capacity(42).expect("capacity"), 42);
    assert!(chunk_capacity(u64::MAX).is_err());
}

#[tokio::test]
async fn signed_transfer_rejects_invalid_headers_and_sources() {
    let temp = tempfile::tempdir().expect("temp");
    let source = temp.path().join("short.bin");
    std::fs::write(&source, b"x").expect("source");
    let client = SignedTransferClient::new();
    let progress = progress();
    let invalid_name = [SignedHeader {
        name: "bad header".into(),
        value: SecretString::new("value").expect("value"),
    }];
    let unused = SecretString::new("http://127.0.0.1:1/signed").expect("url");
    assert!(
        client
            .put_file(&unused, &source, 1, &invalid_name, &progress)
            .await
            .is_err()
    );
    assert!(header_value("line\nbreak").is_err());
    let invalid_value = [SignedHeader {
        name: "x-meta".into(),
        value: SecretString::new("café").expect("unicode value"),
    }];
    assert!(
        signed_headers_with(&invalid_value, |_value| {
            Err(blobyard_core::BlobyardError::from_code(
                ErrorCode::StorageError,
            ))
        })
        .is_err()
    );
    assert!(
        client
            .put_file(&unused, &temp.path().join("missing"), 1, &[], &progress)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn signed_transfer_handles_empty_and_short_bodies_safely() {
    let temp = tempfile::tempdir().expect("temp");
    let source = temp.path().join("source.bin");
    let empty = temp.path().join("empty.bin");
    std::fs::write(&source, b"x").expect("source");
    std::fs::write(&empty, b"").expect("empty");
    let (url, task) = serve(vec![
        reply("200 OK"),
        reply("200 OK"),
        Reply {
            status: "200 OK",
            headers: Vec::new(),
            body: b"bytes".to_vec(),
        },
    ])
    .await;
    let secret = SecretString::new(url).expect("url");
    let client = SignedTransferClient::new();
    let progress = progress();
    client
        .put_file(&secret, &empty, 0, &[], &progress)
        .await
        .expect("empty");
    assert!(
        client
            .put_file(&secret, &source, 2, &[], &progress)
            .await
            .is_err()
    );
    let existing = temp.path().join("existing");
    std::fs::write(&existing, b"old").expect("existing");
    assert!(
        client
            .download(&secret, &existing, &progress)
            .await
            .is_err()
    );
    task.await.expect("server");
}
