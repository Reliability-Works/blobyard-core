#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    ApiCallError, ApiClient, ApiRequest, RawResponse, Transport, TransportFuture,
};
use blobyard_cli::{
    Cli, ConfigLoader, ConfigPaths, Environment, OutputOptions, OutputRenderer, Runner, TokenStore,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use clap::Parser;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[path = "support_fixture.rs"]
mod fixture;

#[derive(Debug, Default)]
/// Read-only map-backed environment fixture.
pub(super) struct TestEnvironment(
    /// Environment values.
    pub(super) HashMap<String, String>,
);

impl Environment for TestEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

impl TestEnvironment {
    /// Uses a closed loopback port so application tests never reach a real API.
    pub(super) fn unavailable_api() -> Self {
        Self(HashMap::from([(
            "BLOBYARD_API_URL".into(),
            "http://127.0.0.1:1/v1".into(),
        )]))
    }

    /// Adds one deterministic environment value.
    pub(super) fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Default)]
struct StoreState {
    token: Option<SecretString>,
    fail_load: bool,
    fail_save: bool,
    fail_delete: bool,
    saves: usize,
    deletes: usize,
}

#[derive(Debug, Default)]
/// In-memory refresh-token store with injected failure controls.
pub(super) struct FakeStore(Mutex<StoreState>);

impl FakeStore {
    /// Creates a store containing a token.
    ///
    /// # Panics
    ///
    /// Panics when the test passes an invalid token fixture.
    #[must_use]
    pub(super) fn with_token(token: &str) -> Self {
        Self(Mutex::new(StoreState {
            token: Some(SecretString::new(token).expect("token")),
            ..StoreState::default()
        }))
    }

    /// Makes subsequent loads fail.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    pub(super) fn fail_load(&self) {
        self.0.lock().expect("store lock").fail_load = true;
    }

    /// Makes subsequent saves fail.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    pub(super) fn fail_save(&self) {
        self.0.lock().expect("store lock").fail_save = true;
    }

    /// Makes subsequent deletes fail.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    pub(super) fn fail_delete(&self) {
        self.0.lock().expect("store lock").fail_delete = true;
    }

    /// Returns a test-only copy of the stored token.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    #[must_use]
    pub(super) fn token(&self) -> Option<String> {
        self.0
            .lock()
            .expect("store lock")
            .token
            .as_ref()
            .map(|value| value.expose_secret().to_owned())
    }

    /// Returns the successful save count.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    #[must_use]
    pub(super) fn saves(&self) -> usize {
        self.0.lock().expect("store lock").saves
    }

    /// Returns the successful delete count.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    #[must_use]
    pub(super) fn deletes(&self) -> usize {
        self.0.lock().expect("store lock").deletes
    }
}

impl TokenStore for FakeStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        let state = self.0.lock().expect("store lock");
        if state.fail_load {
            Err(local_error())
        } else {
            Ok(state.token.clone())
        }
    }

    fn save(&self, token: &SecretString) -> Result<(), BlobyardError> {
        let mut state = self.0.lock().expect("store lock");
        if state.fail_save {
            Err(local_error())
        } else {
            state.saves += 1;
            state.token = Some(token.clone());
            drop(state);
            Ok(())
        }
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        let mut state = self.0.lock().expect("store lock");
        if state.fail_delete {
            Err(local_error())
        } else {
            state.deletes += 1;
            state.token = None;
            drop(state);
            Ok(())
        }
    }
}

fn local_error() -> BlobyardError {
    BlobyardError::from_code(ErrorCode::InternalError)
}

#[derive(Debug)]
/// FIFO one-shot transport that records prepared requests.
pub(super) struct QueueTransport {
    responses: Mutex<VecDeque<Result<RawResponse, ApiCallError>>>,
    requests: Mutex<Vec<ApiRequest>>,
}

impl QueueTransport {
    fn new(responses: Vec<RawResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Returns recorded requests.
    ///
    /// # Panics
    ///
    /// Panics if another test poisoned the fixture mutex.
    #[must_use]
    pub(super) fn requests(&self) -> Vec<ApiRequest> {
        self.requests.lock().expect("request lock").clone()
    }
}

impl Transport for QueueTransport {
    fn send<'a>(&'a self, request: &'a ApiRequest) -> TransportFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("request lock")
                .push(request.clone());
            self.responses
                .lock()
                .expect("response lock")
                .pop_front()
                .expect("queued response")
        })
    }
}

/// Complete runner fixture with owned local state.
pub(super) struct Fixture {
    /// Parsed command.
    pub(super) command: blobyard_cli::Command,
    /// Runner under test.
    pub(super) runner: Runner,
    /// Recording transport.
    pub(super) transport: Arc<QueueTransport>,
    /// In-memory token store.
    pub(super) store: Arc<FakeStore>,
    /// Owned temporary filesystem root.
    pub(super) temp: tempfile::TempDir,
}

/// One loopback signed-storage response.
pub(super) struct SignedReply {
    /// HTTP status and reason.
    pub(super) status: &'static str,
    /// Response headers.
    pub(super) headers: Vec<(&'static str, &'static str)>,
    /// Response body.
    pub(super) body: Vec<u8>,
}

/// Starts a loopback signed-storage server and captures requests.
pub(super) async fn signed_server(
    replies: Vec<SignedReply>,
) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let request = crate::request_capture::capture(&mut socket, "request read").await;
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
    (format!("http://{address}/signed"), task)
}

/// Starts a one-response signed-storage server and runs an action before replying.
pub(super) async fn signed_server_with_action(
    reply: SignedReply,
    action: impl FnOnce() + Send + 'static,
) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let request = crate::request_capture::capture(&mut socket, "request read").await;
        action();
        crate::request_capture::write_response(
            &mut socket,
            reply.status,
            &reply.headers,
            &reply.body,
        )
        .await;
        request
    });
    (format!("http://{address}/signed"), task)
}

/// Creates a successful strict API envelope.
#[must_use]
pub(super) fn ok(data: serde_json::Value, request_id: &str) -> RawResponse {
    let mut body = serde_json::json!({
        "ok": true,
        "data": null,
        "requestId": request_id,
    });
    body["data"] = data;
    RawResponse::new(200, Some(request_id.into()), body.to_string().into_bytes())
}

/// Creates a failed strict API envelope.
#[must_use]
pub(super) fn api_failure(code: ErrorCode, request_id: &str) -> RawResponse {
    let body = serde_json::json!({
        "ok": false,
        "error": { "code": code, "message": "provider detail" },
        "requestId": request_id,
    });
    RawResponse::new(400, Some(request_id.into()), body.to_string().into_bytes())
}

/// Renders and decodes a successful result as JSON.
///
/// # Panics
///
/// Panics if the renderer violates its JSON contract.
#[must_use]
pub(super) fn result_json(result: blobyard_cli::CommandResult) -> serde_json::Value {
    let rendered = OutputRenderer::new(
        OutputOptions::from_flags(&blobyard_cli::GlobalArgs {
            json: true,
            quiet: false,
            verbose: false,
            api_url: None,
            web_yard_origin: None,
            profile: None,
            workspace: None,
            project: None,
            retry_key: None,
        }),
        blobyard_cli::Diagnostics::default(),
    )
    .success(result);
    serde_json::from_str(&rendered.stdout).expect("result json")
}
