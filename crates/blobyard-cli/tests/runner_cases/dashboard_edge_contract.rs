//! Dashboard read, export-download, and destructive lifecycle edge contracts.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_cli::{Diagnostics, GlobalArgs, OutputOptions, OutputRenderer};
use blobyard_core::ErrorCode;

#[tokio::test]
async fn dashboard_reads_use_their_dedicated_endpoints() {
    for (args, endpoint, data) in [
        (
            vec!["blobyard", "billing", "show"],
            Endpoint::GetBilling,
            serde_json::json!({ "plan": "solo" }),
        ),
        (
            vec!["blobyard", "account", "export", "show"],
            Endpoint::GetAccountExport,
            serde_json::json!({ "status": "ready" }),
        ),
        (
            vec!["blobyard", "account", "delete", "show"],
            Endpoint::GetAccountDeletion,
            serde_json::json!({ "status": "none" }),
        ),
    ] {
        let fixture = Fixture::new(
            &args,
            vec![ok(data.clone(), "req_dashboard_read")],
            Some("token"),
            None,
        );
        let output = result_json(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect("dashboard read"),
        );
        assert_eq!(output["data"], data);
        assert_eq!(fixture.transport.requests()[0].endpoint(), endpoint);
    }
}

#[tokio::test]
async fn account_export_download_returns_only_the_issued_part_url() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "account",
            "export",
            "download",
            "export_1",
            "--part-number",
            "2",
        ],
        vec![ok(
            serde_json::json!({ "downloadUrl": "https://storage.example/part-2" }),
            "req_export_download",
        )],
        Some("token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("export part download");
    let rendered = OutputRenderer::new(
        OutputOptions::from_flags(&GlobalArgs {
            json: false,
            quiet: false,
            verbose: false,
            api_url: None,
            web_yard_origin: None,
            profile: None,
            workspace: None,
            project: None,
            retry_key: None,
        }),
        Diagnostics::default(),
    )
    .success(result);
    assert_eq!(rendered.stdout, "https://storage.example/part-2\n");
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::DownloadAccountExport);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "exportId": "export_1", "partNumber": 2 }))
    );
    assert_eq!(requests[0].idempotency_key(), None);

    let missing_url = Fixture::new(
        &["blobyard", "account", "export", "download", "export_1"],
        vec![ok(serde_json::json!({}), "req_export_download_missing")],
        Some("token"),
        None,
    );
    assert_eq!(
        missing_url
            .runner
            .execute(&missing_url.command)
            .await
            .expect_err("missing export URL")
            .code(),
        ErrorCode::InternalError
    );
}

#[tokio::test]
async fn account_export_request_rejects_invalid_lifecycle_status_results() {
    for data in [
        serde_json::json!({ "exportId": "export_1", "status": "ready" }),
        serde_json::json!({ "exportId": "export_1" }),
        serde_json::json!({ "exportId": "export_1", "status": 1 }),
    ] {
        let fixture = Fixture::new(
            &["blobyard", "account", "export", "request"],
            vec![ok(data, "req_export_invalid_status")],
            Some("token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid account export status")
                .code(),
            ErrorCode::InternalError
        );
        let requests = fixture.transport.requests();
        assert_eq!(requests[0].endpoint(), Endpoint::RequestAccountExport);
        assert_eq!(requests[0].body(), Some(&serde_json::json!({})));
    }
}

#[tokio::test]
async fn deletion_retry_requires_force_and_uses_the_fixed_confirmation() {
    let unconfirmed = Fixture::new(
        &["blobyard", "account", "delete", "retry"],
        Vec::new(),
        Some("token"),
        None,
    );
    assert_eq!(
        unconfirmed
            .runner
            .execute(&unconfirmed.command)
            .await
            .expect_err("retry confirmation")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(unconfirmed.transport.requests().is_empty());

    let confirmed = Fixture::new(
        &["blobyard", "account", "delete", "retry", "--force"],
        vec![ok(
            serde_json::json!({ "jobId": "job_1", "status": "queued" }),
            "req_deletion_retry",
        )],
        Some("token"),
        None,
    );
    confirmed
        .runner
        .execute(&confirmed.command)
        .await
        .expect("retry deletion");
    let requests = confirmed.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::RetryAccountDeletion);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "confirmation": "DELETE MY ACCOUNT" }))
    );
    assert_eq!(requests[0].idempotency_key(), None);
}

#[tokio::test]
async fn deletion_completion_rejects_malformed_confirmation_tokens() {
    for token in ["short".to_owned(), format!("{}!", "a".repeat(42))] {
        let fixture = Fixture::new(
            &[
                "blobyard", "account", "delete", "complete", &token, "--force",
            ],
            Vec::new(),
            Some("token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("malformed deletion token")
                .code(),
            ErrorCode::InvalidRequest
        );
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn deletion_completion_propagates_remote_mutation_failures() {
    let token = "a".repeat(43);
    let fixture = Fixture::new(
        &[
            "blobyard", "account", "delete", "complete", &token, "--force",
        ],
        vec![api_failure(
            ErrorCode::ProviderUnavailable,
            "req_complete_failure",
        )],
        Some("token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("remote mutation failure");

    assert_eq!(error.code(), ErrorCode::ProviderUnavailable);
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CompleteAccountDeletion);
    assert!(requests[0].idempotency_key().is_some());
}

#[tokio::test]
async fn billing_plan_validation_rejects_ambiguous_seat_contracts() {
    for args in [
        vec!["blobyard", "billing", "checkout", "solo", "--seats", "1"],
        vec!["blobyard", "billing", "checkout", "team"],
        vec!["blobyard", "billing", "checkout", "team", "--seats", "0"],
        vec!["blobyard", "billing", "update", "team", "--seats", "101"],
    ] {
        let fixture = Fixture::new(&args, Vec::new(), Some("token"), None);
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid plan seat contract")
                .code(),
            ErrorCode::InvalidRequest
        );
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn deletion_preparation_requires_a_confirmation_token_result() {
    let fixture = Fixture::new(
        &["blobyard", "account", "delete", "prepare"],
        vec![ok(serde_json::json!({}), "req_prepare_missing")],
        Some("token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("missing confirmation token")
            .code(),
        ErrorCode::InternalError
    );
}

#[tokio::test]
async fn deletion_preparation_requires_a_recovery_capability_result() {
    let fixture = Fixture::new(
        &["blobyard", "account", "delete", "prepare"],
        vec![ok(
            serde_json::json!({
                "confirmationToken": "a".repeat(43),
                "expiresAt": "2026-07-15T12:00:00Z",
            }),
            "req_prepare_missing_recovery",
        )],
        Some("token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("missing recovery capability")
            .code(),
        ErrorCode::InternalError
    );
}
