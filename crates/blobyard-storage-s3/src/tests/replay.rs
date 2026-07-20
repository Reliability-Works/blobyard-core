use super::support::{ReplayEvent, SdkBody, TestResult};
use crate::replay::StaticReplayClient;
use crate::transport::{RequestBody, S3Request, S3Transport};
use blobyard_contract::StorageError;
use http::{HeaderMap, Method, Request, Response};

fn event(
    method: Method,
    uri: &str,
    header: Option<&str>,
    body: SdkBody,
) -> Result<ReplayEvent, http::Error> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(value) = header {
        request = request.header("x-expected", value);
    }
    Ok(ReplayEvent::new(
        request.body(body)?,
        Response::builder().status(200).body(SdkBody::empty())?,
    ))
}

fn request(
    method: Method,
    uri: &str,
    header: Option<&str>,
    body: RequestBody,
) -> Result<S3Request, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    if let Some(value) = header {
        headers.insert("x-expected", value.parse()?);
    }
    Ok(S3Request {
        method,
        url: uri.parse()?,
        headers,
        body,
    })
}

#[tokio::test]
async fn replay_records_each_request_mismatch() -> TestResult {
    let bytes = SdkBody::from(&b"expected"[..]);
    let replay = StaticReplayClient::new(vec![
        event(
            Method::POST,
            "http://localhost/method",
            None,
            SdkBody::empty(),
        )?,
        event(
            Method::GET,
            "http://localhost/expected",
            None,
            SdkBody::empty(),
        )?,
        event(
            Method::GET,
            "http://localhost/header",
            Some("yes"),
            SdkBody::empty(),
        )?,
        event(Method::GET, "http://localhost/body", None, bytes)?,
    ]);
    replay
        .send(request(
            Method::GET,
            "http://localhost/method",
            None,
            RequestBody::Empty,
        )?)
        .await?;
    replay
        .send(request(
            Method::GET,
            "http://localhost/actual",
            None,
            RequestBody::Empty,
        )?)
        .await?;
    replay
        .send(request(
            Method::GET,
            "http://localhost/header",
            Some("no"),
            RequestBody::Empty,
        )?)
        .await?;
    replay
        .send(request(
            Method::GET,
            "http://localhost/body",
            None,
            RequestBody::Bytes(b"actual".to_vec()),
        )?)
        .await?;
    assert!(std::panic::catch_unwind(|| replay.relaxed_requests_match()).is_err());
    Ok(())
}

#[tokio::test]
async fn replay_records_unexpected_and_unconsumed_requests() -> TestResult {
    let unexpected = StaticReplayClient::new(Vec::new());
    assert_eq!(
        unexpected
            .send(request(
                Method::GET,
                "http://localhost/unexpected",
                None,
                RequestBody::Empty
            )?)
            .await
            .err(),
        Some(StorageError::Unavailable)
    );
    assert!(std::panic::catch_unwind(|| unexpected.relaxed_requests_match()).is_err());

    let unconsumed = StaticReplayClient::new(vec![event(
        Method::GET,
        "http://localhost/unconsumed",
        None,
        SdkBody::empty(),
    )?]);
    assert!(std::panic::catch_unwind(|| unconsumed.relaxed_requests_match()).is_err());
    Ok(())
}

#[tokio::test]
async fn replay_propagates_unreadable_request_bodies() -> TestResult {
    let missing = tempfile::tempdir()?.path().join("missing");
    let unreadable = StaticReplayClient::new(vec![event(
        Method::GET,
        "http://localhost/file",
        None,
        SdkBody::empty(),
    )?]);
    assert_eq!(
        unreadable
            .send(request(
                Method::GET,
                "http://localhost/file",
                None,
                RequestBody::File(missing),
            )?)
            .await
            .err(),
        Some(StorageError::Unavailable)
    );
    Ok(())
}
