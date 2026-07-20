#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{ExchangeRequest, exchange_at};
use crate::{api, error::ApiError, oidc, transfers};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
};
use blobyard_contract::{GithubOidcIdentity, ProjectRecord};
use blobyard_core::Slug;
use futures_util::future::BoxFuture;
use std::sync::Arc;

const OPERATOR_TOKEN: &str = "secret";

/// Isolated router that exercises CI handlers through the normal library build.
pub struct CiFixture {
    router: Router,
    transfer: transfers::test_seams::TransferFixture,
}

/// Builds a CI fixture whose operator has the CI-management scope.
#[must_use]
pub fn fixture() -> CiFixture {
    fixture_with_scopes(&["ci:manage", "object:read", "object:write", "tokens:manage"])
}

/// Builds a CI fixture whose operator cannot manage CI trusts.
#[must_use]
pub fn fixture_without_ci_management() -> CiFixture {
    fixture_with_scopes(&["object:read"])
}

/// Exercises the normal-library exchange clock-failure boundary.
pub async fn clock_failure_status() -> StatusCode {
    let transfer = transfers::test_seams::fixture(&["ci:manage"]);
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        "Bearer aaa.bbb.clock".parse().expect("authorization"),
    );
    exchange_at(
        &transfer.state,
        &headers,
        Ok(Json(ExchangeRequest {
            actions: vec!["upload".to_owned()],
            project: Slug::new("project").expect("project slug"),
            workspace: Some(Slug::new("fixture").expect("workspace slug")),
        })),
        Err(ApiError::internal()),
    )
    .await
    .err()
    .expect("clock failure")
    .into_response()
    .status()
}

impl CiFixture {
    /// Returns the exact router backed by the fixture's durable state.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Returns the fixture operator token.
    #[must_use]
    pub const fn operator_token(&self) -> &'static str {
        OPERATOR_TOKEN
    }

    /// Removes CI trust storage to force provider-failure responses.
    pub fn break_ci_trusts(&self) {
        self.transfer.break_ci_trusts();
    }

    /// Removes machine-session storage to force trust-revocation failure.
    pub fn break_ci_revoke(&self) {
        self.transfer.break_ci_revoke();
    }

    /// Removes workspace lookup storage to force initial trust-lookup failure.
    pub fn break_workspace_listing(&self) {
        self.transfer.break_workspace_listing();
    }

    /// Corrupts one machine session so server authentication must reject it.
    pub fn corrupt_machine_project(&self, raw_token: &str) {
        self.transfer.corrupt_machine_project(raw_token);
    }
}

fn fixture_with_scopes(scopes: &[&str]) -> CiFixture {
    let mut transfer = transfers::test_seams::fixture(scopes);
    transfer.state.oidc_verifier = Arc::new(DeterministicVerifier);
    transfer
        .state
        .repository
        .create_project(&ProjectRecord {
            id: "project_other".to_owned(),
            workspace_id: transfer.state.default_workspace.id.clone(),
            name: "Other".to_owned(),
            slug: Slug::new("other").expect("other project slug"),
        })
        .expect("other project");
    let router = api::router_with_state(transfer.state.clone());
    CiFixture { router, transfer }
}

struct DeterministicVerifier;

impl oidc::GithubOidcVerifier for DeterministicVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        audience: &'a str,
        now_ms: u64,
    ) -> BoxFuture<'a, Result<GithubOidcIdentity, oidc::OidcVerificationError>> {
        Box::pin(async move {
            match token.split('.').next() {
                Some("invalid") => Err(oidc::OidcVerificationError::Invalid),
                Some("unavailable") => Err(oidc::OidcVerificationError::ProviderUnavailable),
                _ => Ok(GithubOidcIdentity {
                    audience: audience.to_owned(),
                    repository: "reliability-works/blobyard-core".to_owned(),
                    git_ref: "refs/heads/main".to_owned(),
                    workflow_path: ".github/workflows/release.yml".to_owned(),
                    workflow_ref: if token.starts_with("invalid-record.") {
                        "main".to_owned()
                    } else {
                        "refs/heads/main".to_owned()
                    },
                    environment: None,
                    run_id: token.to_owned(),
                    run_attempt: Some("1".to_owned()),
                    sha: Some("a".repeat(40)),
                    expires_at_ms: now_ms.saturating_add(600_000),
                }),
            }
        })
    }
}
