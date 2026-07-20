#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::{
    oidc::{GithubOidcVerifier, OidcVerificationError},
    transfers::test_seams,
};
use axum::{extract::State, http::StatusCode, response::IntoResponse};
use blobyard_contract::{GithubOidcIdentity, LocalCiTrustRecord, NewCiAuditEvent, ci_audit_event};
use blobyard_testkit::{ci_trust, github_oidc_identity};
use futures_util::future::BoxFuture;
use std::sync::Arc;

struct Verifier {
    failure: Option<OidcVerificationError>,
}

impl GithubOidcVerifier for Verifier {
    fn verify<'a>(
        &'a self,
        _token: &'a str,
        audience: &'a str,
        now_ms: u64,
    ) -> BoxFuture<'a, Result<GithubOidcIdentity, OidcVerificationError>> {
        Box::pin(async move {
            if let Some(error) = self.failure {
                return Err(error);
            }
            Ok(github_oidc_identity(audience, "12345", now_ms + 600_000))
        })
    }
}

fn headers(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value.parse().expect("authorization"));
    headers
}

fn request(actions: &[&str]) -> ExchangeRequest {
    ExchangeRequest {
        actions: actions.iter().map(|value| (*value).to_owned()).collect(),
        project: Slug::new("project").expect("project slug"),
        workspace: Some(Slug::new("fixture").expect("workspace slug")),
    }
}

fn state(failure: Option<OidcVerificationError>) -> test_seams::TransferFixture {
    let mut fixture = test_seams::fixture(&["ci:manage"]);
    let trust = ci_trust(
        "trust_fixture",
        &fixture.principal.workspace_id,
        Some(&fixture.project.id),
        &fixture.state.public_origin,
        1,
    );
    fixture
        .state
        .repository
        .create_ci_trust(&trust, &trust_event(&fixture, &trust))
        .expect("create trust");
    fixture.state.oidc_verifier = Arc::new(Verifier { failure });
    fixture
}

fn trust_event(fixture: &test_seams::TransferFixture, trust: &LocalCiTrustRecord) -> NewAuditEvent {
    ci_audit_event(NewCiAuditEvent {
        id: "audit_trust".to_owned(),
        workspace_id: trust.workspace_id.clone(),
        actor: fixture.principal.id.clone(),
        action: "ci.trust_created".to_owned(),
        request_id: "request_trust".to_owned(),
        target_type: "ci_trust".to_owned(),
        target_id: trust.id.clone(),
        repository: trust.repository.clone(),
        created_at_ms: trust.created_at_ms,
    })
}

#[tokio::test]
async fn exchanges_once_and_returns_only_the_machine_token_contract() {
    let fixture = state(None);
    let response = exchange(
        State(fixture.state.clone()),
        headers("Bearer aaa.bbb.ccc"),
        Ok(Json(request(&["upload"]))),
    )
    .await
    .expect("exchange");
    let json = serde_json::to_value(response.0).expect("response JSON");
    assert_eq!(json["data"]["scopes"], serde_json::json!(["upload"]));
    assert_eq!(json["data"]["expiresInSeconds"], 600);
    assert!(
        json["data"]["accessToken"]
            .as_str()
            .is_some_and(|value| value.starts_with("byd_ci_"))
    );
    assert_eq!(json["data"].as_object().map(serde_json::Map::len), Some(3));

    let mut default_workspace = request(&["upload"]);
    default_workspace.workspace = None;
    let default_response = exchange(
        State(fixture.state.clone()),
        headers("Bearer aaa.bbb.default"),
        Ok(Json(default_workspace)),
    )
    .await
    .expect("default workspace exchange");
    assert_eq!(
        serde_json::to_value(default_response.0).expect("default response JSON")["data"]["scopes"],
        serde_json::json!(["upload"])
    );

    let replay = exchange(
        State(fixture.state),
        headers("Bearer aaa.bbb.ccc"),
        Ok(Json(request(&["upload"]))),
    )
    .await
    .err()
    .expect("replayed assertion");
    assert_eq!(replay.into_response().status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_untrusted_actions_and_verifier_failures() {
    let fixture = state(None);
    let forbidden = exchange(
        State(fixture.state),
        headers("Bearer aaa.bbb.ccc"),
        Ok(Json(request(&["download"]))),
    )
    .await
    .err()
    .expect("untrusted action");
    assert_eq!(forbidden.into_response().status(), StatusCode::FORBIDDEN);

    for (project, workspace) in [("other-project", "fixture"), ("project", "other-workspace")] {
        let fixture = state(None);
        let mut wrong_target = request(&["upload"]);
        wrong_target.project = Slug::new(project).expect("project slug");
        wrong_target.workspace = Some(Slug::new(workspace).expect("workspace slug"));
        let error = exchange(
            State(fixture.state),
            headers("Bearer aaa.bbb.target"),
            Ok(Json(wrong_target)),
        )
        .await
        .err()
        .expect("wrong target");
        assert_eq!(error.into_response().status(), StatusCode::FORBIDDEN);
    }

    for (failure, expected) in [
        (OidcVerificationError::Invalid, StatusCode::UNAUTHORIZED),
        (
            OidcVerificationError::ProviderUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ] {
        let fixture = state(Some(failure));
        let error = exchange(
            State(fixture.state),
            headers("Bearer aaa.bbb.ccc"),
            Ok(Json(request(&["upload"]))),
        )
        .await
        .err()
        .expect("verifier failure");
        assert_eq!(error.into_response().status(), expected);
    }
}

#[test]
fn rejects_malformed_assertions_and_emits_retry_after() {
    for value in ["", "Bearer opaque", "Basic aaa.bbb.ccc"] {
        assert!(oidc_assertion(&headers(value)).is_err());
    }
    let response = response(
        MachineSessionMintResult::RateLimited {
            retry_after_seconds: 17,
        },
        SecretString::new("unused").expect("secret"),
        1,
        "request".to_owned(),
    )
    .err()
    .expect("rate limit")
    .into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok()),
        Some("17")
    );
}

#[tokio::test]
async fn machine_identity_reports_ci_without_elevating_scopes() {
    let fixture = state(None);
    let response = exchange(
        State(fixture.state.clone()),
        headers("Bearer aaa.bbb.ddd"),
        Ok(Json(request(&["upload"]))),
    )
    .await
    .expect("exchange");
    let json = serde_json::to_value(response.0).expect("exchange JSON");
    let raw = json["data"]["accessToken"].as_str().expect("access token");
    let principal = crate::auth::test_seams::authenticate_at(
        &fixture.state,
        raw,
        transfer_grants::now_ms().expect("current time"),
    )
    .expect("machine principal");
    let identity = crate::api::who_am_i(State(fixture.state.clone()), principal)
        .await
        .expect("machine identity");
    let identity = serde_json::to_value(identity.0).expect("identity JSON");
    assert_eq!(identity["data"]["principalType"], "ci");
    assert_eq!(identity["data"]["scopes"], serde_json::json!(["upload"]));
    assert!(identity["data"]["email"].is_null());

    let trust = fixture
        .state
        .repository
        .list_ci_trusts(&fixture.principal.workspace_id)
        .expect("trusts")
        .into_iter()
        .next()
        .expect("trust");
    let now_ms = transfer_grants::now_ms().expect("current time");
    let mut revoke_event = trust_event(&fixture, &trust);
    revoke_event.id = "audit_trust_revoke".to_owned();
    revoke_event.action = "ci.trust_revoked".to_owned();
    revoke_event.request_id = "request_trust_revoke".to_owned();
    revoke_event.created_at_ms = now_ms;
    assert!(
        fixture
            .state
            .repository
            .revoke_ci_trust(&trust.id, &trust.workspace_id, now_ms, &revoke_event,)
            .expect("revoke trust")
    );
    assert!(crate::auth::test_seams::authenticate_at(&fixture.state, raw, now_ms).is_err());
}

#[path = "api_ci_exchange_limit_tests.rs"]
mod limits;
