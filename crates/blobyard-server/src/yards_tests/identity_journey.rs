#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    mutate,
    session_support::{body, browser_request, setup, sign_in},
};
use crate::contract_test_support::{response_json, send};
use axum::http::{StatusCode, header};

#[tokio::test]
async fn live_identity_recomputes_management_policy_and_grant_state() {
    let session = setup().await;
    let signed_in = sign_in(&session, "/").await;
    let empty = identity(&session, &signed_in.cookie, &[]).await;
    assert_eq!(empty["managementRole"], serde_json::Value::Null);
    assert_eq!(empty["appRoles"], serde_json::json!([]));
    assert_eq!(empty["permissions"], serde_json::json!([]));

    assert_owner_invariants(&session).await;
    replace_owner(&session).await;

    approve_viewer(&session).await;

    let effective = identity(&session, &signed_in.cookie, &[]).await;
    assert_eq!(effective["userId"], session.user_id);
    assert_eq!(effective["yardId"], session.yard_id);
    assert_eq!(effective["managementRole"], "admin");
    assert_eq!(effective["appRoles"], serde_json::json!(["viewer"]));
    assert_eq!(
        effective["permissions"],
        serde_json::json!(["content.read"])
    );
    assert_eq!(effective["groups"], serde_json::json!([]));

    assert_policy_removal_is_live(&session, &signed_in.cookie).await;
}

async fn assert_owner_invariants(session: &super::session_support::SessionFixture) {
    let non_owner_bootstrap = send(
        &session.fixture,
        "POST",
        "/v1/yards/management-roles/set",
        &serde_json::to_vec(&serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
            "role": "admin",
        }))
        .expect("bootstrap request"),
        false,
    )
    .await;
    assert_eq!(non_owner_bootstrap.status(), StatusCode::CONFLICT);
    mutate(
        &session.fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
            "role": "owner",
        }),
    )
    .await;
    let listed = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/management-roles?yardId={}", session.yard_id),
            &[],
            false,
        )
        .await,
    )
    .await;
    assert_eq!(listed["data"]["items"][0]["role"], "owner");
    assert_eq!(listed["data"]["items"][0]["userId"], session.user_id);
    assert_last_owner_cannot_be_removed(session).await;
}

async fn assert_last_owner_cannot_be_removed(session: &super::session_support::SessionFixture) {
    for (path, role) in [
        ("/v1/yards/management-roles/set", Some("developer")),
        ("/v1/yards/management-roles/revoke", None),
    ] {
        let mut request = serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
        });
        if let Some(role) = role {
            request["role"] = serde_json::Value::String(role.to_owned());
        }
        let response = send(
            &session.fixture,
            "POST",
            path,
            &serde_json::to_vec(&request).expect("owner request"),
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}

async fn replace_owner(session: &super::session_support::SessionFixture) {
    let created = response_json(
        send(
            &session.fixture,
            "POST",
            "/v1/users",
            br#"{"displayName":"Backup Owner","workspace":"fixture"}"#,
            false,
        )
        .await,
    )
    .await;
    let backup_user_id = created["data"]["user"]["id"]
        .as_str()
        .expect("backup user ID");
    mutate(
        &session.fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": backup_user_id,
            "role": "owner",
        }),
    )
    .await;
    mutate(
        &session.fixture,
        "/v1/yards/management-roles/revoke",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
        }),
    )
    .await;
    mutate(
        &session.fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
            "role": "admin",
        }),
    )
    .await;
}

async fn approve_viewer(session: &super::session_support::SessionFixture) {
    set_policy(
        session,
        serde_json::json!({
            "defaultRole": null,
            "roles": {
                "viewer": {
                    "inherits": [],
                    "permissions": ["content.read"],
                },
            },
        }),
    )
    .await;
    let current = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/application-policy?yardId={}", session.yard_id),
            &[],
            false,
        )
        .await,
    )
    .await;
    assert_eq!(current["data"]["policy"]["revision"], 1);
    assert_eq!(
        current["data"]["policy"]["roles"]["viewer"]["permissions"],
        serde_json::json!(["content.read"])
    );
    mutate(
        &session.fixture,
        "/v1/yards/access/roles",
        serde_json::json!({
            "yardId": session.yard_id,
            "grantId": session.grant_id,
            "appRoles": ["viewer"],
        }),
    )
    .await;
}

async fn assert_policy_removal_is_live(
    session: &super::session_support::SessionFixture,
    cookie: &str,
) {
    set_policy(
        session,
        serde_json::json!({
            "defaultRole": null,
            "roles": {
                "editor": {
                    "inherits": [],
                    "permissions": ["content.write"],
                },
            },
        }),
    )
    .await;
    let removed = identity(session, cookie, &[]).await;
    assert_eq!(removed["appRoles"], serde_json::json!([]));
    assert_eq!(removed["permissions"], serde_json::json!([]));

    let access = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/access?yardId={}", session.yard_id),
            &[],
            false,
        )
        .await,
    )
    .await;
    assert_eq!(
        access["data"]["grants"][0]["appRoles"],
        serde_json::json!(["viewer"])
    );
}

#[tokio::test]
async fn identity_endpoint_is_same_origin_get_only_and_conceals_denial() {
    let session = setup().await;
    let signed_in = sign_in(&session, "/").await;

    for (method, cookie, headers) in [
        ("POST", Some(signed_in.cookie.as_str()), Vec::new()),
        ("GET", None, Vec::new()),
        (
            "GET",
            Some(signed_in.cookie.as_str()),
            vec![("origin", "https://foreign.example")],
        ),
    ] {
        let response = browser_request(
            &session.fixture,
            method,
            "/.blobyard/session/identity",
            &format!("{}:8787", session.host),
            &headers,
            "",
            cookie,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.headers().get(header::LOCATION).is_none());
    }

    mutate(
        &session.fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": session.yard_id, "visibility": "public" }),
    )
    .await;
    let response = browser_request(
        &session.fixture,
        "GET",
        "/.blobyard/session/identity",
        &format!("{}:8787", session.host),
        &[],
        "",
        Some(&signed_in.cookie),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn identity(
    session: &super::session_support::SessionFixture,
    cookie: &str,
    headers: &[(&str, &str)],
) -> serde_json::Value {
    let response = browser_request(
        &session.fixture,
        "GET",
        "/.blobyard/session/identity",
        &format!("{}:8787", session.host),
        headers,
        "",
        Some(cookie),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    serde_json::from_slice(&body(response).await).expect("identity")
}

async fn set_policy(session: &super::session_support::SessionFixture, policy: serde_json::Value) {
    let mut request = policy;
    request["yardId"] = serde_json::Value::String(session.yard_id.clone());
    request["sourceManifestDigest"] = serde_json::Value::String("a".repeat(64));
    mutate(&session.fixture, "/v1/yards/application-policy", request).await;
}
