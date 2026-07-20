//! Share and retention runner behavior over the typed API seam.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_cli::{Diagnostics, GlobalArgs, OutputOptions, OutputRenderer};
use blobyard_core::ErrorCode;

fn output_flags(quiet: bool) -> GlobalArgs {
    GlobalArgs {
        json: false,
        quiet,
        verbose: false,
        api_url: None,
        web_yard_origin: None,
        profile: None,
        workspace: None,
        project: None,
        retry_key: None,
    }
}

fn share_fixture() -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "share",
            "blobyard://team/mobile/app.zip?version=2",
            "--expires",
            "7d",
            "--notify",
            "dev@example.com",
        ],
        vec![ok(
            serde_json::json!({
                "id": "share_1",
                "shareUrl": "https://blobyard.com/s/raw-capability",
                "expiresAt": "2026-07-16T00:00:00Z",
                "notificationStatus": "queued"
            }),
            "req_share",
        )],
        Some("ci-token"),
        None,
    )
}

async fn share_result(fixture: &Fixture) -> blobyard_cli::CommandResult {
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("create share")
}

#[tokio::test]
async fn creates_a_share_for_an_existing_uri_and_returns_capability_once() {
    let human_fixture = share_fixture();
    let human = OutputRenderer::new(
        OutputOptions::from_flags(&output_flags(false)),
        Diagnostics::default(),
    )
    .success(share_result(&human_fixture).await);
    assert_eq!(
        human.stdout,
        "Share URL: https://blobyard.com/s/raw-capability\n"
    );
    let json_fixture = share_fixture();
    assert_eq!(
        result_json(share_result(&json_fixture).await)["data"]["shareUrl"],
        "https://blobyard.com/s/raw-capability"
    );
    let quiet_fixture = share_fixture();
    let quiet = OutputRenderer::new(
        OutputOptions::from_flags(&output_flags(true)),
        Diagnostics::default(),
    )
    .success(share_result(&quiet_fixture).await);
    assert!(quiet.stdout.is_empty());
    let requests = json_fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CreateShare);
    assert_eq!(requests[0].idempotency_key(), None);
}

#[tokio::test]
async fn share_rejects_invalid_durations_before_api_access() {
    for (target, duration) in [
        ("blobyard://team/mobile/app.zip", "0d"),
        ("blobyard://team/mobile/app.zip", "7w"),
        ("blobyard://team/mobile/app.zip", "d"),
    ] {
        let fixture = Fixture::new(
            &["blobyard", "share", target, "--expires", duration],
            Vec::new(),
            Some("ci-token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid share")
                .code(),
            ErrorCode::InvalidRequest
        );
        assert!(fixture.transport.requests().is_empty());
    }

    for email in ["missing-at", "@example.com", "dev@", "a@b@example.com"] {
        let fixture = Fixture::new(
            &[
                "blobyard",
                "share",
                "blobyard://team/mobile/app.zip",
                "--notify",
                email,
            ],
            Vec::new(),
            Some("ci-token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid notification email")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}

#[tokio::test]
async fn shows_sets_and_clears_retention_with_explicit_match_fields() {
    let policy = serde_json::json!({
        "keepLatest": 20,
        "branchGlob": "main",
        "pathGlob": "builds/**"
    });
    let show = Fixture::new(
        &[
            "blobyard",
            "retention",
            "show",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        vec![ok(policy.clone(), "req_show")],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        result_json(show.runner.execute(&show.command).await.expect("show"))["data"]["keepLatest"],
        20
    );

    let set = Fixture::new(
        &[
            "blobyard",
            "retention",
            "set",
            "--latest",
            "20",
            "--branch",
            "main",
            "--path",
            "builds/**",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        vec![ok(policy, "req_set")],
        Some("ci-token"),
        None,
    );
    set.runner
        .execute(&set.command)
        .await
        .expect("set retention");
    let requests = set.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::SetRetention);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({
            "workspace": "team",
            "project": "app",
            "keepLatest": 20,
            "branch": "main",
            "path": "builds/**",
        }))
    );

    let clear = Fixture::new(
        &[
            "blobyard",
            "retention",
            "clear",
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        vec![ok(serde_json::json!({ "cleared": true }), "req_clear")],
        Some("ci-token"),
        None,
    );
    let cleared = clear
        .runner
        .execute(&clear.command)
        .await
        .expect("clear retention");
    assert_eq!(result_json(cleared)["data"]["cleared"], true);
    assert_eq!(
        clear.transport.requests()[0].endpoint(),
        Endpoint::ClearRetention
    );
}

#[tokio::test]
async fn retention_rejects_missing_scope_without_mutation() {
    let missing = Fixture::new(
        &["blobyard", "retention", "show", "--workspace", "team"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        missing
            .runner
            .execute(&missing.command)
            .await
            .expect_err("project required")
            .code(),
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn retention_rejects_invalid_globs_without_mutation() {
    for glob in ["", "line\nbreak"] {
        let fixture = Fixture::new(
            &[
                "blobyard",
                "retention",
                "set",
                "--latest",
                "2",
                "--branch",
                glob,
                "--workspace",
                "team",
                "--project",
                "app",
            ],
            Vec::new(),
            Some("ci-token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid glob")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
    let long_glob = "x".repeat(257);
    let fixture = Fixture::new(
        &[
            "blobyard",
            "retention",
            "set",
            "--latest",
            "2",
            "--path",
            &long_glob,
            "--workspace",
            "team",
            "--project",
            "app",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert!(fixture.runner.execute(&fixture.command).await.is_err());
}
