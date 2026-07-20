use super::super::LoginPort;
use crate::{Cli, Command, ConfigLoader, ConfigPaths, Environment, Runner, TokenStore};
use blobyard_api_client::{
    ApiCallError, ApiClient, ApiRequest, RawResponse, Transport, TransportFuture,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use clap::Parser;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug)]
pub(super) struct RecordingPort(Mutex<PortState>);

#[derive(Debug, Default)]
pub(super) struct PortState {
    pub instructions: Vec<(String, String)>,
    pub opened: Vec<String>,
    pub waits: Vec<Duration>,
}

impl Default for RecordingPort {
    fn default() -> Self {
        Self(Mutex::new(PortState::default()))
    }
}

impl RecordingPort {
    pub(super) fn state(&self) -> std::sync::MutexGuard<'_, PortState> {
        self.0.lock().expect("login port lock")
    }
}

impl LoginPort for RecordingPort {
    fn open_browser(&self, url: &str) -> bool {
        self.0
            .lock()
            .expect("login port lock")
            .opened
            .push(url.to_owned());
        false
    }

    fn present(&self, verification_uri: &str, user_code: &SecretString) {
        self.0.lock().expect("login port lock").instructions.push((
            verification_uri.to_owned(),
            user_code.expose_secret().to_owned(),
        ));
    }

    fn wait(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.0.lock().expect("login port lock").waits.push(duration);
        Box::pin(async {})
    }
}

#[derive(Debug, Default)]
pub(in crate::runner) struct FakeStore {
    state: Mutex<StoreState>,
}

#[derive(Debug, Default)]
struct StoreState {
    fail_save: bool,
    saves: usize,
    token: Option<SecretString>,
}

impl FakeStore {
    pub(super) fn fail_save(&self) {
        self.state.lock().expect("store lock").fail_save = true;
    }

    pub(super) fn saves(&self) -> usize {
        self.state.lock().expect("store lock").saves
    }

    pub(super) fn token(&self) -> Option<String> {
        self.state
            .lock()
            .expect("store lock")
            .token
            .as_ref()
            .map(|value| value.expose_secret().to_owned())
    }
}

impl TokenStore for FakeStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        Ok(self.state.lock().expect("store lock").token.clone())
    }

    fn save(&self, token: &SecretString) -> Result<(), BlobyardError> {
        let mut state = self.state.lock().expect("store lock");
        if state.fail_save {
            return Err(BlobyardError::from_code(ErrorCode::InternalError));
        }
        state.saves += 1;
        state.token = Some(token.clone());
        drop(state);
        Ok(())
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        self.state.lock().expect("store lock").token = None;
        Ok(())
    }
}

#[derive(Debug)]
pub(in crate::runner) struct QueueTransport {
    requests: Mutex<Vec<ApiRequest>>,
    responses: Mutex<VecDeque<RawResponse>>,
}

impl QueueTransport {
    fn new(responses: Vec<RawResponse>) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into()),
        }
    }

    pub(in crate::runner) fn requests(&self) -> Vec<ApiRequest> {
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
                .ok_or_else(|| {
                    ApiCallError::new(
                        BlobyardError::from_code(ErrorCode::InternalError),
                        blobyard_api_client::RetryAdvice::Never,
                    )
                })
        })
    }
}

#[derive(Debug, Default)]
struct EmptyEnvironment;

impl Environment for EmptyEnvironment {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

pub(in crate::runner) struct Fixture {
    pub(super) command: Command,
    pub(in crate::runner) runner: Runner,
    pub(in crate::runner) store: Arc<FakeStore>,
    pub(in crate::runner) transport: Arc<QueueTransport>,
    _temp: tempfile::TempDir,
}

impl Fixture {
    pub(in crate::runner) fn new(args: &[&str], responses: Vec<RawResponse>) -> Self {
        let cli = Cli::try_parse_from(args).expect("command grammar");
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ConfigLoader::new(
            ConfigPaths::new(temp.path(), temp.path().join("user/config.toml")),
            &EmptyEnvironment,
        )
        .load(&cli.global)
        .expect("resolved config");
        let store = Arc::new(FakeStore::default());
        let transport = Arc::new(QueueTransport::new(responses));
        let runner = Runner::new(ApiClient::new(transport.clone()), config, store.clone());
        Self {
            command: cli.command,
            runner,
            store,
            transport,
            _temp: temp,
        }
    }

    pub(super) fn with_port(self, port: Arc<RecordingPort>) -> Self {
        Self {
            runner: self.runner.with_login_port(port),
            ..self
        }
    }
}

pub(in crate::runner) fn ok(data: &serde_json::Value, request_id: &str) -> RawResponse {
    let body = serde_json::json!({ "ok": true, "data": data, "requestId": request_id });
    RawResponse::new(200, Some(request_id.into()), body.to_string())
}

pub(in crate::runner) fn api_failure(
    code: ErrorCode,
    status: u16,
    request_id: &str,
) -> RawResponse {
    let body = serde_json::json!({
        "ok": false,
        "error": { "code": code, "message": "safe test failure" },
        "requestId": request_id
    });
    RawResponse::new(status, Some(request_id.into()), body.to_string())
}

pub(super) fn start_response(interval: u16, uri: &str) -> RawResponse {
    ok(
        &serde_json::json!({
            "deviceCode": "device-code-fixture",
            "userCode": "ABCD-2345",
            "verificationUri": uri,
            "expiresAt": "2030-01-01T00:10:00.000Z",
            "pollIntervalSeconds": interval
        }),
        "req_start",
    )
}

pub(super) fn poll_response(
    status: &str,
    tokens: Option<serde_json::Value>,
    request_id: &str,
) -> RawResponse {
    let mut data = serde_json::json!({ "status": status });
    if let Some(tokens) = tokens {
        data["tokens"] = tokens;
    }
    ok(&data, request_id)
}

pub(super) fn approved_response() -> RawResponse {
    poll_response(
        "approved",
        Some(serde_json::json!({
            "accessToken": "access-token-fixture",
            "refreshToken": "refresh-token-fixture",
            "expiresInSeconds": 900
        })),
        "req_approved",
    )
}

#[test]
fn credential_fixture_load_and_delete_are_deterministic() {
    let store = FakeStore::default();
    assert_eq!(store.load().expect("load"), None);
    store.delete().expect("delete");
    assert_eq!(store.token(), None);
}

#[tokio::test]
async fn empty_transport_queue_returns_a_safe_internal_error() {
    let transport = QueueTransport::new(Vec::new());
    let request = ApiRequest::new(blobyard_api_client::Endpoint::Health);
    let error = transport.send(&request).await.expect_err("empty queue");
    assert_eq!(error.error().code(), ErrorCode::InternalError);
}
