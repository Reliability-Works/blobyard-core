//! Bearer-authenticated workspace, billing, and account lifecycle adapters.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

#[path = "admin_contract_support.rs"]
pub(super) mod contract_support;

use super::support::{Fixture, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;
use contract_support::{human_output, human_stdout};

const DELETION_RETRY_KEY: &str = "delete.complete.2026-07-15";

fn deletion_complete_args(token: &str) -> [&str; 8] {
    [
        "blobyard",
        "account",
        "delete",
        "complete",
        token,
        "--force",
        "--retry-key",
        DELETION_RETRY_KEY,
    ]
}

#[tokio::test]
async fn workspace_rename_uses_selected_scope_without_a_retry_key() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "workspaces",
            "rename",
            "Platform Team",
            "--workspace",
            "team",
        ],
        vec![ok(
            serde_json::json!({ "id": "workspace_1", "name": "Platform Team", "slug": "team" }),
            "req_rename",
        )],
        Some("token"),
        None,
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("rename");
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::RenameWorkspace);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "name": "Platform Team", "workspace": "team" }))
    );
    assert_eq!(requests[0].idempotency_key(), None);
}

#[tokio::test]
async fn hosted_billing_sessions_render_the_url_and_preserve_the_api_contract() {
    for (args, endpoint, expected_body, label, url) in [
        (
            vec!["blobyard", "billing", "checkout", "team", "--seats", "4"],
            Endpoint::CreateBillingCheckout,
            serde_json::json!({ "plan": "team", "seats": 4 }),
            "Billing checkout URL",
            "https://checkout.stripe.test/session/cs_checkout",
        ),
        (
            vec!["blobyard", "billing", "portal"],
            Endpoint::CreateBillingPortal,
            serde_json::json!({}),
            "Billing portal URL",
            "https://billing.stripe.test/session/bps_portal",
        ),
        (
            vec!["blobyard", "billing", "storage", "checkout", "2"],
            Endpoint::CreateStorageCheckout,
            serde_json::json!({ "storageBlockCount": 2 }),
            "Storage checkout URL",
            "https://checkout.stripe.test/session/cs_storage_checkout",
        ),
        (
            vec!["blobyard", "billing", "storage", "update", "3"],
            Endpoint::CreateStorageUpdate,
            serde_json::json!({ "storageBlockCount": 3 }),
            "Storage update URL",
            "https://billing.stripe.test/session/bps_storage_update",
        ),
        (
            vec!["blobyard", "billing", "update", "solo"],
            Endpoint::CreateBillingSubscriptionUpdate,
            serde_json::json!({ "plan": "solo" }),
            "Subscription update URL",
            "https://billing.stripe.test/session/bps_subscription_update",
        ),
    ] {
        assert_hosted_billing_session(&args, endpoint, &expected_body, label, url).await;
    }
}

async fn assert_hosted_billing_session(
    args: &[&str],
    endpoint: Endpoint,
    expected_body: &serde_json::Value,
    label: &str,
    url: &str,
) {
    let fixture = Fixture::new(
        args,
        vec![ok(serde_json::json!({ "url": url }), "req_dashboard")],
        Some("token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("dashboard");
    let rendered = human_output(result);
    assert_eq!(rendered.stdout, format!("{label}: {url}\n"));
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), endpoint);
    assert_eq!(requests[0].body(), Some(expected_body));
    assert_eq!(
        requests[0].idempotency_key().is_some(),
        endpoint.supports_idempotency()
    );

    let json_fixture = Fixture::new(
        args,
        vec![ok(serde_json::json!({ "url": url }), "req_dashboard_json")],
        Some("token"),
        None,
    );
    let json = result_json(
        json_fixture
            .runner
            .execute(&json_fixture.command)
            .await
            .expect("dashboard JSON"),
    );
    assert_eq!(json["data"], serde_json::json!({ "url": url }));
}

#[tokio::test]
async fn account_export_request_uses_the_dedicated_endpoint() {
    let fixture = Fixture::new(
        &["blobyard", "account", "export", "request"],
        vec![ok(
            serde_json::json!({ "exportId": "export_1", "status": "queued" }),
            "req_export",
        )],
        Some("token"),
        None,
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("export request");
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::RequestAccountExport);
    assert_eq!(requests[0].body(), Some(&serde_json::json!({})));
    assert!(requests[0].idempotency_key().is_some());
}

#[tokio::test]
async fn hosted_billing_sessions_require_the_url_field() {
    let fixture = Fixture::new(
        &["blobyard", "billing", "portal"],
        vec![ok(serde_json::json!({}), "req_missing_url")],
        Some("token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("missing hosted-session URL");
    assert_eq!(error.code(), ErrorCode::ProviderUnavailable);
    assert_eq!(
        fixture.transport.requests()[0].endpoint(),
        Endpoint::CreateBillingPortal
    );
}

#[tokio::test]
async fn deletion_preparation_returns_the_single_use_confirmation() {
    let token = "a".repeat(43);
    let recovery_token = "r".repeat(43);
    let prepare = Fixture::new(
        &["blobyard", "account", "delete", "prepare"],
        vec![ok(
            serde_json::json!({
                "confirmationToken": token,
                "expiresAt": "2026-07-15T12:00:00Z",
                "recoveryToken": recovery_token,
            }),
            "req_prepare",
        )],
        Some("token"),
        None,
    );
    let prepared = result_json(
        prepare
            .runner
            .execute(&prepare.command)
            .await
            .expect("prepare"),
    );
    assert_eq!(prepared["data"]["confirmationToken"], "a".repeat(43));
    assert_eq!(prepared["data"]["recoveryToken"], "r".repeat(43));
    assert_eq!(
        prepare.transport.requests()[0].endpoint(),
        Endpoint::PrepareAccountDeletion
    );
    assert!(prepare.transport.requests()[0].idempotency_key().is_some());
}

#[tokio::test]
async fn account_lifecycle_human_output_matches_durable_status() {
    for (args, data, expected) in [
        (
            vec!["blobyard", "account", "export", "request"],
            serde_json::json!({ "exportId": "export_1", "status": "running" }),
            "Account export already running.\n",
        ),
        (
            vec!["blobyard", "account", "delete", "retry", "--force"],
            serde_json::json!({ "jobId": "job_1", "status": "running" }),
            "Account deletion running.\n",
        ),
    ] {
        let fixture = Fixture::new(&args, vec![ok(data, "req_lifecycle")], Some("token"), None);
        let output = human_stdout(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect("account lifecycle output"),
        );
        assert_eq!(output, expected);
    }
}

#[tokio::test]
async fn deletion_completion_requires_force_before_transport() {
    let unconfirmed = Fixture::new(
        &["blobyard", "account", "delete", "complete", &"a".repeat(43)],
        Vec::new(),
        Some("token"),
        None,
    );
    assert_eq!(
        unconfirmed
            .runner
            .execute(&unconfirmed.command)
            .await
            .expect_err("force")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(unconfirmed.transport.requests().is_empty());
}

#[tokio::test]
async fn deletion_completion_reuses_an_opaque_retry_key() {
    let complete_token = "b".repeat(43);
    let complete = Fixture::new(
        &deletion_complete_args(&complete_token),
        vec![ok(
            serde_json::json!({ "jobId": "job_1", "status": "queued" }),
            "req_complete",
        )],
        Some("token"),
        None,
    );
    complete
        .runner
        .execute(&complete.command)
        .await
        .expect("complete");
    let requests = complete.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CompleteAccountDeletion);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "confirmationToken": complete_token }))
    );
    let replay = Fixture::new(
        &deletion_complete_args(&complete_token),
        vec![ok(
            serde_json::json!({ "jobId": "job_1", "status": "queued" }),
            "req_complete_replay",
        )],
        Some("token"),
        None,
    );
    replay
        .runner
        .execute(&replay.command)
        .await
        .expect("replayed completion");
    let retry_idempotency_key = replay.transport.requests()[0]
        .idempotency_key()
        .expect("retry key")
        .to_owned();
    assert_eq!(
        requests[0].idempotency_key(),
        Some(retry_idempotency_key.as_str())
    );
    assert!(!retry_idempotency_key.contains(DELETION_RETRY_KEY));
    assert!(!retry_idempotency_key.contains(&complete_token));
}
