#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    TransportSelection, prepare, profile_name, read_bootstrap_token, render, required_api,
    select_transport, selected_token_store,
};
use crate::{
    CommandResult, ConfigPaths, Diagnostics, FileTokenStore, GlobalArgs, OutputOptions,
    OutputRenderer, ProfileAddArgs, TokenStore,
};
use blobyard_api_client::{
    ApiCallError, ApiClientConfig, ApiRequest, RawResponse, RetryAdvice, Transport, TransportFuture,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use std::io::{Error, Read};
use std::sync::Arc;

#[derive(Debug)]
struct FailingReader(std::io::ErrorKind);

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::from(self.0))
    }
}

#[test]
fn bootstrap_input_is_bounded_trimmed_and_read_atomically() {
    let token = "b".repeat(43);
    let terminated = format!("{token}\r\n");
    let mut terminated_reader = terminated.as_bytes();
    let parsed = read_bootstrap_token(&mut terminated_reader).expect("newline-terminated token");
    assert_eq!(parsed.expose_secret(), token);

    let oversized = "b".repeat(16_385);
    let mut empty = std::io::empty();
    let mut oversized_reader = oversized.as_bytes();
    let mut failing = FailingReader(std::io::ErrorKind::PermissionDenied);
    let errors = [
        read_bootstrap_token(&mut empty).expect_err("empty token"),
        read_bootstrap_token(&mut oversized_reader).expect_err("oversized token"),
        read_bootstrap_token(&mut failing).expect_err("read failure"),
    ];
    for error in errors {
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert_eq!(
            error.message(),
            "Standard input must contain one valid bootstrap token."
        );
    }
}

#[test]
fn profile_and_api_guards_return_specific_failures() {
    assert_eq!(
        profile_name("bad profile")
            .expect_err("invalid profile")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        profile_name("cloud").expect_err("reserved profile").code(),
        ErrorCode::Conflict
    );
    assert_eq!(
        required_api(&crate::GlobalArgs {
            json: false,
            quiet: false,
            verbose: false,
            api_url: None,
            web_yard_origin: None,
            profile: None,
            workspace: None,
            project: None,
            retry_key: None,
        })
        .expect_err("missing API")
        .code(),
        ErrorCode::InvalidRequest
    );
    let secret = SecretString::new("b".repeat(43)).expect("secret fixture");
    assert_eq!(secret.expose_secret().len(), 43);
}

#[test]
fn production_transport_and_warning_rendering_are_explicit() {
    let api = ApiClientConfig::new("http://localhost:8787").expect("API URL");
    assert!(select_transport(&api, TransportSelection::Automatic).is_ok());
    let options = OutputOptions::from_flags(&crate::GlobalArgs {
        json: false,
        quiet: false,
        verbose: false,
        api_url: None,
        web_yard_origin: None,
        profile: None,
        workspace: None,
        project: None,
        retry_key: None,
    });

    let output = render(
        OutputRenderer::new(options, Diagnostics::default()),
        Ok((
            CommandResult::local(serde_json::json!({ "ok": true }), "Profile added."),
            Some("credential warning"),
        )),
    );
    assert_eq!(output.exit_code, 0);
    assert!(output.stderr.contains("credential warning"));

    let output = render(
        OutputRenderer::new(options, Diagnostics::default()),
        Err(blobyard_core::BlobyardError::from_code(
            ErrorCode::InvalidRequest,
        )),
    );
    assert_eq!(output.exit_code, ErrorCode::InvalidRequest.exit_code());
}

#[tokio::test]
async fn preparation_maps_transport_and_store_failures_before_writing_config() {
    for (index, transport, store) in [
        (
            0,
            TransportSelection::Failure(BlobyardError::from_code(ErrorCode::InternalError)),
            None,
        ),
        (
            1,
            TransportSelection::Ready(Arc::new(FixtureTransport { fails: true })),
            None,
        ),
        (
            2,
            TransportSelection::Ready(Arc::new(FixtureTransport { fails: false })),
            Some(Arc::new(RejectingStore) as Arc<dyn TokenStore>),
        ),
    ] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let paths = ConfigPaths::new(
            temporary.path(),
            temporary.path().join(format!("user-{index}/config.toml")),
        );
        let result = prepare(
            &global(),
            &ProfileAddArgs {
                name: "local".to_owned(),
                token_stdin: true,
            },
            &paths,
            store,
            SecretString::new("b".repeat(43)).expect("bootstrap token"),
            transport,
        )
        .await;
        assert_eq!(
            result.expect_err("injected preparation failure").code(),
            ErrorCode::InternalError
        );
        assert!(!paths.user_config().exists());
    }
}

fn global() -> GlobalArgs {
    GlobalArgs {
        json: false,
        quiet: false,
        verbose: false,
        api_url: Some("http://localhost:8787".to_owned()),
        web_yard_origin: None,
        profile: None,
        workspace: None,
        project: None,
        retry_key: None,
    }
}

#[derive(Debug)]
struct FixtureTransport {
    fails: bool,
}

impl Transport for FixtureTransport {
    fn send<'a>(&'a self, _request: &'a ApiRequest) -> TransportFuture<'a> {
        Box::pin(async move {
            if self.fails {
                return Err(ApiCallError::new(
                    BlobyardError::from_code(ErrorCode::InternalError),
                    RetryAdvice::Never,
                ));
            }
            Ok(RawResponse::new(
                200,
                Some("req_fixture".to_owned()),
                format!(
                    r#"{{"ok":true,"data":{{"accessToken":"{}","scopes":[],"webYardOrigin":"http://localhost:8787","workspace":"default"}},"requestId":"req_fixture"}}"#,
                    "a".repeat(43)
                ),
            ))
        })
    }
}

#[derive(Debug)]
struct RejectingStore;

impl TokenStore for RejectingStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        Ok(None)
    }

    fn save(&self, _token: &SecretString) -> Result<(), BlobyardError> {
        Err(BlobyardError::from_code(ErrorCode::InternalError))
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        Ok(())
    }
}

#[test]
fn profile_store_selection_uses_injected_or_platform_authority_without_exposing_paths() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let paths = ConfigPaths::new(temporary.path(), temporary.path().join("user/config.toml"));
    let profile = profile_name("local").expect("profile");
    let injected = selected_token_store(
        &profile,
        &paths,
        Some(Arc::new(FileTokenStore::new(
            temporary.path().join("injected.token"),
        ))),
    );
    assert!(injected.warning().is_none());

    let selected = selected_token_store(&profile, &paths, None);
    let debug = format!("{selected:?}");
    assert!(debug.starts_with("SelectedTokenStore"));
    assert!(!debug.contains(&temporary.path().display().to_string()));
}
