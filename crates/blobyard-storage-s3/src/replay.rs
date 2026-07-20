use crate::transport::{S3Request, S3Response, S3Transport, TransportFuture};
use blobyard_contract::StorageError;
use http::{HeaderMap, Method, Request, Response};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TestBody(Vec<u8>);

impl TestBody {
    pub(crate) const fn empty() -> Self {
        Self(Vec::new())
    }
}

impl From<&str> for TestBody {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

impl From<String> for TestBody {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

impl From<&[u8]> for TestBody {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

pub(crate) struct ReplayEvent {
    request: Request<TestBody>,
    response: Response<TestBody>,
}

impl ReplayEvent {
    pub(crate) const fn new(request: Request<TestBody>, response: Response<TestBody>) -> Self {
        Self { request, response }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedRequest {
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl CapturedRequest {
    pub(crate) const fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

#[derive(Default)]
struct ReplayState {
    events: VecDeque<ReplayEvent>,
    actual: Vec<CapturedRequest>,
    failures: Vec<String>,
}

#[derive(Clone, Default)]
pub(crate) struct StaticReplayClient {
    state: Arc<Mutex<ReplayState>>,
}

impl StaticReplayClient {
    pub(crate) fn new(events: Vec<ReplayEvent>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReplayState {
                events: events.into(),
                actual: Vec::new(),
                failures: Vec::new(),
            })),
        }
    }

    pub(crate) fn actual_requests(&self) -> std::vec::IntoIter<CapturedRequest> {
        lock(&self.state).actual.clone().into_iter()
    }

    pub(crate) fn relaxed_requests_match(&self) {
        self.assert_requests_match(&[]);
    }

    pub(crate) fn assert_requests_match(&self, _ignored: &[()]) {
        let state = lock(&self.state);
        assert!(state.events.is_empty(), "unconsumed replay events");
        assert!(state.failures.is_empty(), "{}", state.failures.join("\n"));
    }
}

impl S3Transport for StaticReplayClient {
    fn send(&self, request: S3Request) -> TransportFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let body = request.body.clone().into_bytes().await?;
            let mut state = lock(&state);
            let Some(event) = state.events.pop_front() else {
                state.failures.push("unexpected request".to_owned());
                return Err(StorageError::Unavailable);
            };
            let captured = capture(&request, body);
            if let Some(failure) = compare(&event.request, &captured) {
                state.failures.push(failure);
            }
            state.actual.push(captured);
            drop(state);
            let (parts, body) = event.response.into_parts();
            Ok(S3Response::from_bytes(parts.status, parts.headers, body.0))
        })
    }
}

fn capture(request: &S3Request, body: Vec<u8>) -> CapturedRequest {
    CapturedRequest {
        method: request.method.clone(),
        uri: request.url.as_str().to_owned(),
        headers: request.headers.clone(),
        body,
    }
}

fn compare(expected: &Request<TestBody>, actual: &CapturedRequest) -> Option<String> {
    if expected.method() != actual.method {
        return Some(format!(
            "method mismatch: expected {}, got {}",
            expected.method(),
            actual.method
        ));
    }
    if expected.uri().to_string() != actual.uri {
        return Some(format!(
            "URI mismatch: expected {}, got {}",
            expected.uri(),
            actual.uri
        ));
    }
    for (name, value) in expected.headers() {
        if actual.headers.get(name) != Some(value) {
            return Some(format!("header mismatch for {name}"));
        }
    }
    if !expected.body().0.is_empty() && expected.body().0 != actual.body {
        return Some("request body mismatch".to_owned());
    }
    None
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
