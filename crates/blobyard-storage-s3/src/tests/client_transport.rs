use super::support::{ReplayEvent, SdkBody, TestResult};
use crate::S3Credentials;
use crate::client::S3Client;
use crate::replay::StaticReplayClient;
use crate::transport::{RequestBody, ReqwestTransport, S3Request, S3Response, S3Transport};
use blobyard_contract::StorageError;
use blobyard_core::SecretString;
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use std::fmt::Write as _;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

async fn server(
    response: Vec<u8>,
) -> Result<(Url, tokio::task::JoinHandle<Vec<u8>>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let task = tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return Vec::new();
        };
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let Ok(read) = socket.read(&mut buffer).await else {
                return request;
            };
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if complete_request(&request) {
                break;
            }
        }
        let _ignored = socket.write_all(&response).await;
        let _ignored = socket.shutdown().await;
        request
    });
    Ok((format!("http://{address}/object").parse()?, task))
}

fn complete_request(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let length = headers.lines().find_map(|line| {
        line.to_ascii_lowercase()
            .strip_prefix("content-length: ")
            .and_then(|value| value.parse::<usize>().ok())
    });
    if let Some(length) = length {
        return request.len() >= header_end + 4 + length;
    }
    let chunked = headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"));
    !chunked || request[header_end + 4..].ends_with(b"0\r\n\r\n")
}

fn http_response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for (name, value) in headers {
        let _ignored = write!(response, "{name}: {value}\r\n");
    }
    response.push_str("\r\n");
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn credentials(token: Option<&str>) -> Result<S3Credentials, Box<dyn std::error::Error>> {
    Ok(S3Credentials::new(
        SecretString::new("access")?,
        SecretString::new("secret")?,
        token.map(SecretString::new).transpose()?,
    ))
}

async fn send_virtual_host_request(
    replay: &StaticReplayClient,
    key: Option<&str>,
    token: Option<&str>,
) -> TestResult {
    let client = S3Client::new(
        Arc::new(replay.clone()),
        "http://localhost:9000".parse()?,
        "us-east-1".to_owned(),
        "bucket".to_owned(),
        credentials(token)?,
        false,
    );
    client
        .send(
            Method::GET,
            key,
            &[],
            HeaderMap::new(),
            RequestBody::Empty,
            S3Client::empty_hash(),
        )
        .await?;
    replay.relaxed_requests_match();
    Ok(())
}

#[tokio::test]
async fn reqwest_transport_sends_each_body_and_preserves_response_metadata() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let file = temporary.path().join("body.bin");
    std::fs::write(&file, b"file-body")?;
    let bodies = [
        (RequestBody::Empty, Vec::new()),
        (
            RequestBody::Bytes(b"bytes-body".to_vec()),
            b"bytes-body".to_vec(),
        ),
        (RequestBody::File(file), b"file-body".to_vec()),
    ];
    for (body, expected) in bodies {
        let response = http_response(
            "201 Created",
            &[("Content-Length", "2"), ("X-Test", "present")],
            b"ok",
        );
        let (url, captured) = server(response).await?;
        let transport = ReqwestTransport::new()?;
        let response = transport
            .send(S3Request {
                method: Method::POST,
                url,
                headers: HeaderMap::new(),
                body,
            })
            .await?;
        assert_eq!(response.status, StatusCode::CREATED);
        assert_eq!(
            response.headers.get("x-test"),
            Some(&HeaderValue::from_static("present"))
        );
        assert_eq!(response.collect(2).await?, b"ok");
        let request = captured.await?;
        assert!(
            expected.is_empty()
                || request
                    .windows(expected.len())
                    .any(|window| window == expected)
        );
    }
    Ok(())
}

#[tokio::test]
async fn reqwest_transport_maps_body_connection_stream_and_bound_failures() -> TestResult {
    let transport = ReqwestTransport::new()?;
    let missing = tempfile::tempdir()?.path().join("missing");
    let request = S3Request {
        method: Method::PUT,
        url: "http://127.0.0.1:1/missing".parse()?,
        headers: HeaderMap::new(),
        body: RequestBody::File(missing),
    };
    assert!(matches!(
        transport.send(request).await,
        Err(StorageError::Unavailable)
    ));

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    drop(listener);
    let request = S3Request {
        method: Method::GET,
        url: format!("http://{address}/closed").parse()?,
        headers: HeaderMap::new(),
        body: RequestBody::Empty,
    };
    assert!(matches!(
        transport.send(request).await,
        Err(StorageError::Unavailable)
    ));

    let response = http_response("200 OK", &[("Content-Length", "4")], b"x");
    let (url, captured) = server(response).await?;
    let response = transport
        .send(S3Request {
            method: Method::GET,
            url,
            headers: HeaderMap::new(),
            body: RequestBody::Empty,
        })
        .await?;
    assert_eq!(response.collect(8).await, Err(StorageError::Unavailable));
    captured.await?;

    let oversized = S3Response::from_items(StatusCode::OK, vec![Ok(b"abc".to_vec())]);
    assert_eq!(oversized.collect(2).await, Err(StorageError::Unavailable));
    Ok(())
}

#[tokio::test]
async fn client_builds_virtual_host_requests() -> TestResult {
    let expected = ReplayEvent::new(
        Request::builder()
            .method(Method::GET)
            .uri("http://bucket.localhost:9000/key")
            .header("x-amz-security-token", "session")
            .body(SdkBody::empty())?,
        Response::builder().status(200).body(SdkBody::empty())?,
    );
    let replay = StaticReplayClient::new(vec![expected]);
    send_virtual_host_request(&replay, Some("key"), Some("session")).await
}

#[tokio::test]
async fn client_accepts_a_virtual_host_request_without_an_object_key() -> TestResult {
    let expected = ReplayEvent::new(
        Request::builder()
            .method(Method::GET)
            .uri("http://bucket.localhost:9000/")
            .body(SdkBody::empty())?,
        Response::builder().status(200).body(SdkBody::empty())?,
    );
    let replay = StaticReplayClient::new(vec![expected]);
    send_virtual_host_request(&replay, None, None).await
}

#[tokio::test]
async fn client_rejects_invalid_urls_and_headers() -> TestResult {
    let replay = Arc::new(StaticReplayClient::new(Vec::new()));
    let invalid_cases = [
        ("file:///tmp", "bucket", true),
        ("file:///tmp", "bucket", false),
        ("mailto:test@example.com", "bucket", true),
        ("http://localhost", "bad bucket", false),
    ];
    for (endpoint, bucket, path_style) in invalid_cases {
        let client = S3Client::new(
            replay.clone(),
            endpoint.parse()?,
            "us-east-1".to_owned(),
            bucket.to_owned(),
            credentials(None)?,
            path_style,
        );
        assert!(matches!(
            client
                .send(
                    Method::GET,
                    None,
                    &[],
                    HeaderMap::new(),
                    RequestBody::Empty,
                    S3Client::empty_hash(),
                )
                .await,
            Err(StorageError::InvalidInput)
        ));
    }

    let client = S3Client::new(
        replay,
        "http://localhost".parse()?,
        "us-east-1".to_owned(),
        "bucket".to_owned(),
        credentials(None)?,
        true,
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-invalid", HeaderValue::from_bytes(&[0xff])?);
    assert!(matches!(
        client
            .send(
                Method::GET,
                None,
                &[],
                headers,
                RequestBody::Empty,
                S3Client::empty_hash(),
            )
            .await,
        Err(StorageError::InvalidInput)
    ));
    Ok(())
}
