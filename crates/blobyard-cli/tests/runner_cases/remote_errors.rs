//! Remote and scope failures for every implemented API-backed command.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure};
use blobyard_core::ErrorCode;

const REMOTE_CASES: &[&[&str]] = &[
    &["blobyard", "whoami"],
    &["blobyard", "projects", "list", "--workspace", "team"],
    &[
        "blobyard",
        "projects",
        "create",
        "Builds",
        "--workspace",
        "team",
    ],
    &["blobyard", "ls", "--workspace", "team", "--project", "app"],
    &["blobyard", "rm", "blobyard://team/app/old.zip"],
    &["blobyard", "share", "blobyard://team/app/build.zip"],
    &[
        "blobyard",
        "inbox",
        "create",
        "Logs",
        "--workspace",
        "team",
        "--project",
        "app",
    ],
    &[
        "blobyard",
        "inbox",
        "list",
        "--workspace",
        "team",
        "--project",
        "app",
    ],
    &["blobyard", "inbox", "revoke", "inbox_1"],
    &[
        "blobyard",
        "retention",
        "show",
        "--workspace",
        "team",
        "--project",
        "app",
    ],
    &[
        "blobyard",
        "retention",
        "set",
        "--latest",
        "2",
        "--workspace",
        "team",
        "--project",
        "app",
    ],
    &[
        "blobyard",
        "retention",
        "clear",
        "--workspace",
        "team",
        "--project",
        "app",
    ],
];

#[tokio::test]
async fn every_api_backed_command_propagates_remote_failures() {
    for args in REMOTE_CASES {
        let fixture = Fixture::new(
            args,
            vec![api_failure(ErrorCode::Forbidden, "req_forbidden")],
            Some("ci-token"),
            None,
        );
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("remote failure");
        assert_eq!(error.code(), ErrorCode::Forbidden, "args: {args:?}");
        assert_eq!(fixture.transport.requests().len(), 1, "args: {args:?}");
    }
}

#[tokio::test]
async fn scoped_commands_fail_before_network_access_when_scope_is_missing() {
    let cases: &[&[&str]] = &[
        &["blobyard", "inbox", "create", "Logs"],
        &["blobyard", "inbox", "list", "--workspace", "team"],
        &["blobyard", "retention", "show"],
        &[
            "blobyard",
            "retention",
            "set",
            "--latest",
            "2",
            "--workspace",
            "team",
        ],
        &["blobyard", "retention", "clear", "--workspace", "team"],
    ];
    for args in cases {
        let fixture = Fixture::new(args, Vec::new(), Some("ci-token"), None);
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("scope failure");
        assert_eq!(error.code(), ErrorCode::InvalidRequest, "args: {args:?}");
        assert!(fixture.transport.requests().is_empty(), "args: {args:?}");
    }
}
