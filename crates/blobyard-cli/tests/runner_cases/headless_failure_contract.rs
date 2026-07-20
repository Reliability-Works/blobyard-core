//! Failure propagation and validation contracts for headless commands.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure};
use blobyard_core::ErrorCode;

fn remote_failure_cases() -> Vec<Vec<&'static str>> {
    vec![
        vec!["blobyard", "members", "list", "--workspace", "team"],
        vec![
            "blobyard",
            "tokens",
            "create",
            "CI",
            "--expires-days",
            "7",
            "--scope",
            "audit:read",
        ],
        vec!["blobyard", "account", "export", "download", "export_1"],
        vec!["blobyard", "account", "delete", "prepare"],
        vec!["blobyard", "billing", "show"],
        vec!["blobyard", "account", "export", "request"],
        vec!["blobyard", "workspaces", "list"],
        vec!["blobyard", "workspaces", "create", "Platform"],
        vec![
            "blobyard",
            "workspaces",
            "rename",
            "Platform",
            "--workspace",
            "team",
        ],
    ]
}

#[tokio::test]
async fn headless_adapters_preserve_remote_failures() {
    for args in remote_failure_cases() {
        let fixture = Fixture::new(
            &args,
            vec![api_failure(ErrorCode::InvalidRequest, "req_failure")],
            Some("token"),
            None,
        );
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("remote failure");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert_eq!(fixture.transport.requests().len(), 1);
    }
}

fn invalid_command_cases() -> Vec<Vec<&'static str>> {
    vec![
        vec![
            "blobyard",
            "tokens",
            "create",
            " ",
            "--expires-days",
            "7",
            "--scope",
            "audit:read",
        ],
        vec!["blobyard", "previews", "revoke", "bad id"],
        vec!["blobyard", "shares", "revoke", "bad id"],
        vec![
            "blobyard",
            "workspaces",
            "rename",
            "line\nbreak",
            "--workspace",
            "team",
        ],
        vec!["blobyard", "retention", "overview"],
    ]
}

#[tokio::test]
async fn headless_adapters_reject_invalid_input_before_transport() {
    for args in invalid_command_cases() {
        let fixture = Fixture::new(&args, Vec::new(), Some("token"), None);
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("invalid headless input");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert!(fixture.transport.requests().is_empty());
    }
}
