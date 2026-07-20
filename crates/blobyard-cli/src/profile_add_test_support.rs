#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_api_client::{ApiRequest, RawResponse, Transport, TransportFuture};
use blobyard_cli::{Cli, ConfigLoader, ConfigPaths, Environment, TokenStore};
use blobyard_core::{BlobyardError, SecretString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default)]
struct EmptyEnvironment;

impl Environment for EmptyEnvironment {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

#[derive(Debug, Default)]
pub(super) struct RecordingStore {
    token: Mutex<Option<SecretString>>,
}

impl TokenStore for RecordingStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        Ok(self.token.lock().expect("token lock").clone())
    }

    fn save(&self, token: &SecretString) -> Result<(), BlobyardError> {
        *self.token.lock().expect("token lock") = Some(token.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        *self.token.lock().expect("token lock") = None;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(super) struct RecordingTransport {
    pub(super) requests: Mutex<Vec<ApiRequest>>,
    response: Mutex<Option<RawResponse>>,
}

impl RecordingTransport {
    pub(super) fn bootstrap() -> Self {
        Self::bootstrap_workspace("default")
    }

    pub(super) fn bootstrap_workspace(workspace: &str) -> Self {
        Self::bootstrap_values(workspace, "http://localhost:8787")
    }

    pub(super) fn bootstrap_origin(web_yard_origin: &str) -> Self {
        Self::bootstrap_values("default", web_yard_origin)
    }

    fn bootstrap_values(workspace: &str, web_yard_origin: &str) -> Self {
        Self {
            requests: Mutex::default(),
            response: Mutex::new(Some(RawResponse::new(
                200,
                Some("req_bootstrap".to_owned()),
                format!(
                    r#"{{"ok":true,"data":{{"accessToken":"{}","scopes":["object:read","object:write"],"webYardOrigin":{},"workspace":{}}},"requestId":"req_bootstrap"}}"#,
                    "a".repeat(43),
                    serde_json::to_string(web_yard_origin).expect("Web Yard origin JSON"),
                    serde_json::to_string(workspace).expect("workspace JSON")
                ),
            ))),
        }
    }

    pub(super) fn identity() -> Self {
        Self {
            requests: Mutex::default(),
            response: Mutex::new(Some(RawResponse::new(
                200,
                Some("req_identity".to_owned()),
                br#"{"ok":true,"data":{"principalType":"cli","principalId":"operator_1","displayName":"Local operator","email":null,"scopes":["object:read"],"defaultWorkspace":{"id":"workspace_1","name":"Default","slug":"default"}},"requestId":"req_identity"}"#.to_vec(),
            ))),
        }
    }
}

impl Transport for RecordingTransport {
    fn send<'a>(&'a self, request: &'a ApiRequest) -> TransportFuture<'a> {
        self.requests
            .lock()
            .expect("request lock")
            .push(request.clone());
        let response = self.response.lock().expect("response lock").take();
        Box::pin(async move { Ok(response.expect("one response")) })
    }
}

#[derive(Debug)]
pub(super) struct ConfigBlockingTransport {
    path: PathBuf,
    response: Mutex<Option<RawResponse>>,
}

impl ConfigBlockingTransport {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            response: Mutex::new(
                RecordingTransport::bootstrap()
                    .response
                    .into_inner()
                    .expect("response lock"),
            ),
        }
    }
}

impl Transport for ConfigBlockingTransport {
    fn send<'a>(&'a self, _request: &'a ApiRequest) -> TransportFuture<'a> {
        std::fs::create_dir_all(&self.path).expect("blocking config directory");
        let response = self.response.lock().expect("response lock").take();
        Box::pin(async move { Ok(response.expect("one response")) })
    }
}

pub(super) fn fixture_config(cli: &Cli, cwd: &Path) -> blobyard_cli::ResolvedConfig {
    ConfigLoader::new(
        ConfigPaths::new(cwd, cwd.join("user/config.toml")),
        &EmptyEnvironment,
    )
    .load(&cli.global)
    .expect("config")
}
