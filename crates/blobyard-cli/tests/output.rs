//! Stable human, quiet, JSON, diagnostic, and exit contracts.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_cli::{
    CommandResult, ConfigSource, Diagnostics, GlobalArgs, OutputMode, OutputOptions, OutputRenderer,
};
use blobyard_core::{BlobyardError, ErrorCode};

const fn flags(json: bool, quiet: bool, verbose: bool) -> GlobalArgs {
    GlobalArgs {
        json,
        quiet,
        verbose,
        api_url: None,
        web_yard_origin: None,
        profile: None,
        workspace: None,
        project: None,
        retry_key: None,
    }
}

#[test]
fn derives_all_output_modes_and_json_takes_precedence() {
    let human = OutputOptions::from_flags(&flags(false, false, false));
    assert_eq!(human.mode(), OutputMode::Human);
    assert!(!human.verbose());
    assert_eq!(
        OutputOptions::from_flags(&flags(false, true, false)).mode(),
        OutputMode::Quiet
    );
    let json = OutputOptions::from_flags(&flags(true, true, true));
    assert_eq!(json.mode(), OutputMode::Json);
    assert!(json.verbose());
}

#[test]
fn human_and_quiet_success_outputs_are_stable() {
    let renderer = OutputRenderer::new(
        OutputOptions::from_flags(&flags(false, false, false)),
        Diagnostics::default(),
    );
    let result = renderer.success(CommandResult::local(serde_json::json!({}), "Done."));
    assert_eq!(result.stdout, "Done.\n");
    assert_eq!(result.stderr, "");
    assert_eq!(result.exit_code, 0);

    let already_terminated = renderer.success(CommandResult::local(
        serde_json::Value::Null,
        "line\n".to_owned(),
    ));
    assert_eq!(already_terminated.stdout, "line\n");
    let empty = renderer.success(CommandResult::local(serde_json::Value::Null, ""));
    assert_eq!(empty.stdout, "");

    let quiet = OutputRenderer::new(
        OutputOptions::from_flags(&flags(false, true, false)),
        Diagnostics::default(),
    )
    .success(CommandResult::local(serde_json::json!({}), "hidden"));
    assert_eq!(quiet.stdout, "");
}

#[test]
fn json_success_is_one_document_with_nullable_request_id() {
    let renderer = OutputRenderer::new(
        OutputOptions::from_flags(&flags(true, false, false)),
        Diagnostics::default(),
    );
    let local = renderer.success(CommandResult::local(
        serde_json::json!({ "text": "quote \" and newline\n" }),
        "ignored",
    ));
    assert_eq!(local.stdout.lines().count(), 1);
    let local_json: serde_json::Value = serde_json::from_str(&local.stdout).expect("valid json");
    assert_eq!(local_json["ok"], true);
    assert_eq!(local_json["requestId"], serde_json::Value::Null);

    let remote_result = CommandResult::new(
        serde_json::json!({ "id": "value" }),
        "ignored",
        Some("req_123".into()),
    );
    assert!(format!("{remote_result:?}").contains("req_123"));
    let remote = renderer.success(remote_result);
    let remote_json: serde_json::Value = serde_json::from_str(&remote.stdout).expect("valid json");
    assert_eq!(remote_json["requestId"], "req_123");
}

#[test]
fn partial_failure_preserves_data_and_returns_the_error_exit_code() {
    let renderer = OutputRenderer::new(
        OutputOptions::from_flags(&flags(true, false, false)),
        Diagnostics::default(),
    );
    let result = renderer.success(CommandResult::partial_failure(
        serde_json::json!({ "results": [{ "yard": "docs", "ok": false }] }),
        "Web Yard: docs\nStatus: failed",
        BlobyardError::from_code(ErrorCode::PlanLimit).with_request_id("req_partial"),
    ));
    assert_eq!(result.exit_code, ErrorCode::PlanLimit.exit_code());
    let document: serde_json::Value = serde_json::from_str(&result.stdout).expect("valid json");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"]["results"][0]["yard"], "docs");
    assert_eq!(document["error"]["code"], "PLAN_LIMIT");
    assert_eq!(document["requestId"], "req_partial");
}

#[test]
fn warnings_and_verbose_diagnostics_never_contain_credentials() {
    let diagnostics = Diagnostics::default()
        .with_api("https://api.blobyard.com/v1", ConfigSource::Flag)
        .with_scope(Some(ConfigSource::Environment), Some(ConfigSource::Project))
        .with_token_source("credential_store");
    let renderer = OutputRenderer::new(
        OutputOptions::from_flags(&flags(false, false, true)),
        diagnostics,
    )
    .with_warning("credential file fallback active");
    assert!(format!("{renderer:?}").contains("OutputRenderer"));
    let output = renderer.success(CommandResult::new(
        serde_json::json!({}),
        "Done",
        Some("req_safe".into()),
    ));
    assert!(output.stderr.contains("credential file fallback active"));
    assert!(
        output
            .stderr
            .contains("diagnostic api=https://api.blobyard.com/v1")
    );
    assert!(output.stderr.contains("diagnostic api_source=flag"));
    assert!(
        output
            .stderr
            .contains("diagnostic workspace_source=environment")
    );
    assert!(output.stderr.contains("diagnostic project_source=project"));
    assert!(
        output
            .stderr
            .contains("diagnostic token_source=credential_store")
    );
    assert!(output.stderr.contains("diagnostic request_id=req_safe"));
    assert!(!output.stderr.contains("Bearer"));
}

#[test]
fn failures_use_stable_human_json_and_quiet_contracts() {
    let error = BlobyardError::from_code(ErrorCode::AuthRequired).with_request_id("req_auth");
    let human = OutputRenderer::new(
        OutputOptions::from_flags(&flags(false, false, false)),
        Diagnostics::default(),
    )
    .failure(&error);
    assert_eq!(human.stdout, "");
    assert_eq!(
        human.stderr,
        "[AUTH_REQUIRED] Sign in with blobyard login.\n"
    );
    assert_eq!(human.exit_code, 10);
    assert!(!human.stderr.contains("req_auth"));

    let quiet = OutputRenderer::new(
        OutputOptions::from_flags(&flags(false, true, false)),
        Diagnostics::default(),
    )
    .failure(&error);
    assert!(quiet.stderr.contains("AUTH_REQUIRED"));

    let json = OutputRenderer::new(
        OutputOptions::from_flags(&flags(true, false, true)),
        Diagnostics::default(),
    )
    .with_warning("safe warning")
    .failure(&error);
    assert_eq!(
        json.stderr,
        "safe warning\ndiagnostic request_id=req_auth\n"
    );
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    assert_eq!(document["ok"], false);
    assert_eq!(document["error"]["code"], "AUTH_REQUIRED");
    assert_eq!(document["error"]["message"], "Sign in with blobyard login.");
    assert_eq!(document["requestId"], "req_auth");
}
