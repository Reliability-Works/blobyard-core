#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::YardOidcConfiguration;
use axum::{
    Form, Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use blobyard_core::SecretString;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::IntoFuture,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    time::SystemTime,
};

pub(super) const CLIENT_ID: &str = "loopback-client";
pub(super) const CLIENT_SECRET: &str = "loopback-client-secret";
pub(super) const NONCE: &str = "loopback-nonce";
pub(super) const VERIFIER: &str =
    "vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv";
pub(super) const PUBLIC_ORIGIN: &str = "http://127.0.0.1:8787";
pub(super) const CALLBACK_URI: &str = "http://127.0.0.1:8787/account/yard-oidc/callback";

#[derive(Clone)]
struct ProviderState {
    issuer: String,
    metadata_mode: Arc<AtomicU8>,
    keys_requests: Arc<AtomicUsize>,
}

pub(super) struct LoopbackProvider {
    pub(super) issuer: String,
    metadata_mode: Arc<AtomicU8>,
    keys_requests: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl LoopbackProvider {
    pub(super) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback provider listener");
        let address = listener.local_addr().expect("loopback address");
        let issuer = format!("http://{address}/");
        let metadata_mode = Arc::new(AtomicU8::new(0));
        let keys_requests = Arc::new(AtomicUsize::new(0));
        let state = ProviderState {
            issuer: issuer.clone(),
            metadata_mode: Arc::clone(&metadata_mode),
            keys_requests: Arc::clone(&keys_requests),
        };
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/keys", get(keys))
            .route("/token", post(token_endpoint))
            .route("/userinfo", get(user_info))
            .with_state(state);
        let task = tokio::spawn(axum::serve(listener, router).into_future());
        Self {
            issuer,
            metadata_mode,
            keys_requests,
            task,
        }
    }

    pub(super) fn configuration(&self) -> YardOidcConfiguration {
        YardOidcConfiguration::from_optional(
            Some(self.issuer.clone()),
            Some(CLIENT_ID.to_owned()),
            SecretString::new(CLIENT_SECRET).ok(),
        )
        .expect("configuration")
        .expect("enabled configuration")
    }

    pub(super) fn metadata_mode(&self, mode: u8) {
        self.metadata_mode.store(mode, Ordering::Relaxed);
    }

    pub(super) fn keys_request_count(&self) -> usize {
        self.keys_requests.load(Ordering::Relaxed)
    }
}

impl Drop for LoopbackProvider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn discovery(State(state): State<ProviderState>) -> Response {
    let mode = state.metadata_mode.load(Ordering::Relaxed);
    if mode == 4 {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let mut metadata = serde_json::json!({
        "issuer": state.issuer,
        "authorization_endpoint": format!("{}authorize", state.issuer),
        "token_endpoint": format!("{}token", state.issuer),
        "userinfo_endpoint": format!("{}userinfo", state.issuer),
        "jwks_uri": format!("{}keys", state.issuer),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["HS256"]
    });
    match mode {
        1 => metadata["token_endpoint"] = serde_json::json!("http://identity.example.test/token"),
        2 => {
            metadata["userinfo_endpoint"] =
                serde_json::json!("http://identity.example.test/userinfo");
        }
        3 => {
            metadata
                .as_object_mut()
                .expect("metadata object")
                .remove("token_endpoint");
        }
        5 => {
            metadata
                .as_object_mut()
                .expect("metadata object")
                .remove("userinfo_endpoint");
        }
        6 => {
            metadata["jwks_uri"] =
                serde_json::json!(state.issuer.replacen("http://", "http://user@", 1) + "keys");
        }
        _ => {}
    }
    Json(metadata).into_response()
}

async fn keys(State(state): State<ProviderState>) -> Json<serde_json::Value> {
    state.keys_requests.fetch_add(1, Ordering::Relaxed);
    Json(serde_json::json!({"keys": []}))
}

fn valid_token_request(headers: &HeaderMap, form: &HashMap<String, String>) -> bool {
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}"))
    );
    let authenticated = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(expected.as_str());
    authenticated
        && form
            .get("grant_type")
            .is_some_and(|value| value == "authorization_code")
        && form
            .get("redirect_uri")
            .is_some_and(|value| value == CALLBACK_URI)
        && form
            .get("code_verifier")
            .is_some_and(|value| value == VERIFIER)
}

async fn token_endpoint(
    State(state): State<ProviderState>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if !valid_token_request(&headers, &form) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let code = form.get("code").map_or("", String::as_str);
    if code == "unavailable" {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let access_token = format!("{code}-access");
    if code == "missing-id-token" {
        return Json(serde_json::json!({
            "access_token": access_token,
            "token_type": "Bearer"
        }))
        .into_response();
    }
    if code == "oversized" {
        return Json(serde_json::json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "padding": "x".repeat(4 * 1_024 * 1_024)
        }))
        .into_response();
    }
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("current time")
        .as_secs();
    let mut claims = serde_json::json!({
        "iss": state.issuer,
        "aud": [CLIENT_ID],
        "exp": now + 300,
        "iat": now,
        "nonce": NONCE,
        "sub": "provider-subject"
    });
    apply_claim_variants(&mut claims, code, now);
    if code != "wrong-hash" {
        claims["at_hash"] = serde_json::json!(access_token_hash(&access_token));
    }
    let secret = if code == "bad-signature" {
        "wrong-secret"
    } else {
        CLIENT_SECRET
    };
    Json(serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "id_token": signed_token(&claims, secret)
    }))
    .into_response()
}

fn apply_claim_variants(claims: &mut serde_json::Value, code: &str, now: u64) {
    match code {
        "claims-email" => {
            claims["email"] = serde_json::json!(" Person@Example.Test ");
            claims["email_verified"] = serde_json::json!(true);
        }
        "invalid-subject" => claims["sub"] = serde_json::json!("invalid\nsubject"),
        "future-not-before" => claims["nbf"] = serde_json::json!(now + 300),
        "wrong-hash" => claims["at_hash"] = serde_json::json!("wrong"),
        "wrong-authorized-party" => {
            claims["aud"] = serde_json::json!([CLIENT_ID, "other"]);
            claims["azp"] = serde_json::json!("other");
        }
        _ => {}
    }
}

async fn user_info(headers: HeaderMap) -> Response {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    match authorization {
        "Bearer userinfo-access" => Json(serde_json::json!({
            "sub": "provider-subject",
            "email": "UserInfo@Example.Test",
            "email_verified": true
        }))
        .into_response(),
        "Bearer userinfo-failure-access" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}

fn access_token_hash(access_token: &str) -> String {
    let digest = Sha256::digest(access_token.as_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..digest.len() / 2])
}

pub(super) fn request(uri: &str) -> openidconnect::HttpRequest {
    axum::http::Request::builder()
        .uri(uri)
        .body(Vec::new())
        .expect("HTTP request")
}

fn signed_token(payload: &serde_json::Value, secret: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).expect("token claims"));
    let signing_input = format!("{header}.{payload}");
    let mut signer = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
    signer.update(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(signer.finalize().into_bytes());
    format!("{signing_input}.{signature}")
}
