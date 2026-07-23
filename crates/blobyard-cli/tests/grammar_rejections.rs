//! Command grammar rejection and redaction contract tests.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_cli::Cli;
use clap::Parser;

#[test]
fn rejects_invalid_required_values_and_conflicting_output_flags() {
    let cases: &[&[&str]] = &[
        &["blobyard", "download", "blobyard://studio/default/app.zip"],
        &["blobyard", "retention", "set", "--latest", "0"],
        &["blobyard", "whoami", "--quiet", "--verbose"],
        &["blobyard", "completion", "powershell"],
        &["blobyard", "deploy", "./dist", "--all"],
        &["blobyard", "access", "set-visibility", "docs"],
        &["blobyard", "access", "revoke", "docs"],
        &["blobyard", "access", "grant", "docs"],
        &["blobyard", "whoami", "--retry-key", "invalid key"],
        &[
            "blobyard",
            "profiles",
            "add",
            "local",
            "--api-url",
            "http://localhost:8787",
        ],
    ];

    for args in cases {
        assert!(
            Cli::try_parse_from(*args).is_err(),
            "unexpected grammar: {args:?}"
        );
    }
}

#[test]
fn retry_keys_are_redacted_from_debug_output() {
    let cli = Cli::try_parse_from(["blobyard", "whoami", "--retry-key", "opaque-retry-key"])
        .expect("retry key grammar");
    assert_eq!(format!("{:?}", cli.global.retry_key), "Some([REDACTED])");
}
