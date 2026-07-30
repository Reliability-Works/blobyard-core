#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::start;
use crate::{
    transfers::test_seams,
    yard_oidc_provider::{
        YardOidcAuthorization, YardOidcExchangeFuture, YardOidcProvider, YardOidcProviderError,
        YardOidcVerifiedIdentity,
    },
    yard_session_contracts,
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use blobyard_contract::{NewWebYard, NewYardDeploy, YardVisibility};
use blobyard_core::{SecretString, Slug};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub(super) const NOW: u64 = 100;
const ISSUER: &str = "https://identity.example.test/";

struct RecordedAuthorization {
    state: String,
    nonce: String,
    pkce_verifier: String,
}

pub(super) struct TestProvider {
    authorization: Mutex<Option<RecordedAuthorization>>,
    pub(super) exchange_count: AtomicUsize,
    failure: Option<YardOidcProviderError>,
    provider_subject: &'static str,
    email: &'static str,
}

impl TestProvider {
    pub(super) fn member() -> Self {
        Self {
            authorization: Mutex::new(None),
            exchange_count: AtomicUsize::new(0),
            failure: None,
            provider_subject: "member-subject",
            email: "member@example.test",
        }
    }

    pub(super) fn guest() -> Self {
        Self {
            provider_subject: "guest-subject",
            email: "guest@example.test",
            ..Self::member()
        }
    }

    pub(super) fn missing() -> Self {
        Self {
            email: "missing@example.test",
            ..Self::member()
        }
    }

    pub(super) fn failing() -> Self {
        Self {
            failure: Some(YardOidcProviderError::InvalidResponse),
            ..Self::member()
        }
    }

    pub(super) fn state(&self) -> String {
        self.authorization
            .lock()
            .expect("authorization lock")
            .as_ref()
            .expect("recorded authorization")
            .state
            .clone()
    }
}

impl YardOidcProvider for TestProvider {
    fn authorization_url(
        &self,
        authorization: &YardOidcAuthorization,
    ) -> Result<String, YardOidcProviderError> {
        let recorded = RecordedAuthorization {
            state: authorization.state.expose_secret().to_owned(),
            nonce: authorization.nonce.expose_secret().to_owned(),
            pkce_verifier: authorization.pkce_verifier.expose_secret().to_owned(),
        };
        let location = format!(
            "https://identity.example.test/authorize?state={}",
            recorded.state
        );
        *self.authorization.lock().expect("authorization lock") = Some(recorded);
        Ok(location)
    }

    fn exchange(
        &self,
        code: SecretString,
        nonce: SecretString,
        pkce_verifier: SecretString,
        now_ms: u64,
    ) -> YardOidcExchangeFuture<'_> {
        self.exchange_count.fetch_add(1, Ordering::SeqCst);
        let (recorded_nonce, recorded_verifier) = self
            .authorization
            .lock()
            .expect("authorization lock")
            .as_ref()
            .map(|authorization| {
                (
                    authorization.nonce.clone(),
                    authorization.pkce_verifier.clone(),
                )
            })
            .expect("recorded authorization");
        let valid = code.expose_secret() == "provider-code"
            && nonce.expose_secret() == recorded_nonce
            && pkce_verifier.expose_secret() == recorded_verifier
            && recorded_nonce != recorded_verifier
            && now_ms == NOW + 1;
        let failure = self.failure;
        let provider_subject = self.provider_subject;
        let email = self.email;
        Box::pin(async move {
            if !valid {
                return Err(YardOidcProviderError::InvalidResponse);
            }
            if let Some(failure) = failure {
                return Err(failure);
            }
            Ok(YardOidcVerifiedIdentity {
                issuer: ISSUER.to_owned(),
                provider_subject: provider_subject.to_owned(),
                normalized_email: Some(email.to_owned()),
            })
        })
    }
}

pub(super) struct Fixture {
    pub(super) _transfer: test_seams::TransferFixture,
    pub(super) state: crate::api::AppState,
    pub(super) continuation: SecretString,
    pub(super) provider: Arc<TestProvider>,
    pub(super) host_label: String,
}

pub(super) fn member_fixture() -> Fixture {
    fixture(TestProvider::member(), false)
}

pub(super) fn guest_fixture() -> Fixture {
    fixture(TestProvider::guest(), true)
}

pub(super) fn missing_fixture() -> Fixture {
    fixture(TestProvider::missing(), false)
}

pub(super) fn failure_fixture() -> Fixture {
    fixture(TestProvider::failing(), false)
}

fn fixture(provider: TestProvider, guest: bool) -> Fixture {
    let transfer = test_seams::fixture(&["yard:read"]);
    let state = &transfer.state;
    let host_label = "documentation-123456789-fixture".to_owned();
    let yard = new_yard(state, &host_label);
    state
        .repository
        .start_yard_deploy(
            &yard,
            &new_deploy(&yard),
            &blobyard_testkit::yard_event("yard.created", "web_yard", "yardId", &yard.id, 2),
        )
        .expect("yard");
    let visibility = if guest {
        YardVisibility::Selected
    } else {
        YardVisibility::AnyAuthenticated
    };
    state
        .repository
        .set_yard_visibility(
            &yard.id,
            visibility,
            3,
            &blobyard_testkit::visibility_event(&yard.id, "public", visibility.as_str(), 3),
        )
        .expect("visibility");
    if guest {
        super::guest_test_support::seed_guest(state, &yard);
    } else {
        seed_member(state);
    }
    let continuation =
        yard_session_contracts::issue(&state.yard_continuation_key, &host_label, "/reports", NOW)
            .expect("continuation");
    let provider = Arc::new(provider);
    let mut state = state.clone();
    state.yard_oidc_provider = Some(provider.clone());
    Fixture {
        _transfer: transfer,
        state,
        continuation,
        provider,
        host_label,
    }
}

fn new_yard(state: &crate::api::AppState, host_label: &str) -> NewWebYard {
    NewWebYard {
        id: "yard_oidc_fixture".to_owned(),
        workspace_id: state.default_workspace.id.clone(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("documentation").expect("yard name"),
        host_label: host_label.to_owned(),
        created_at_ms: 2,
    }
}

fn new_deploy(yard: &NewWebYard) -> NewYardDeploy {
    NewYardDeploy {
        id: "deploy_oidc_fixture".to_owned(),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "clientdeploy00000001".to_owned(),
        manifest_root: ".blobyard-yard/yard_oidc_fixture/clientdeploy00000001/".to_owned(),
        deployment_host_label: "documentation-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        created_at_ms: 2,
    }
}

fn seed_member(state: &crate::api::AppState) {
    let user = blobyard_testkit::local_user(
        &state.default_workspace.id,
        "user_oidc_fixture",
        Some("member@example.test".to_owned()),
        4,
    );
    state
        .repository
        .create_local_user(
            &user,
            &blobyard_testkit::login_key("userkey_oidc_fixture", &user.id, 'a', 4),
            &blobyard_testkit::local_user_event(
                "audit_user_oidc_fixture",
                &user,
                "user.created",
                4,
            ),
        )
        .expect("member");
}

pub(super) fn start_request(continuation: &SecretString) -> Request<Body> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("continuation", continuation.expose_secret())
        .finish();
    Request::builder()
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("start request")
}

pub(super) async fn begin(fixture: &Fixture) -> String {
    let response = start(
        &fixture.state,
        "fingerprint",
        start_request(&fixture.continuation),
        Ok(NOW),
    )
    .await
    .expect("start");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let state = fixture.provider.state();
    assert!(crate::yard_oidc_contracts::state_shape(&state));
    assert!(
        response
            .headers()
            .get(header::LOCATION)
            .expect("location")
            .to_str()
            .expect("location text")
            .ends_with(&state)
    );
    state
}
