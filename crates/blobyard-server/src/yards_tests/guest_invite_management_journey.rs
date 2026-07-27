use super::{
    mutate,
    session_support::{SessionFixture, path_and_query},
};
use crate::{
    auth::Principal,
    contract_test_support::{response_json, send},
};
use axum::{http::StatusCode, response::Response};
use blobyard_api_client::CreateYardGuestInviteRequest;
use blobyard_testkit::FixtureExecutionTracker;

pub(super) async fn approve_application_roles(session: &SessionFixture) {
    mutate(
        &session.fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": session.user_id,
            "role": "owner"
        }),
    )
    .await;
    mutate(
        &session.fixture,
        "/v1/yards/application-policy",
        serde_json::json!({
            "yardId": session.yard_id,
            "sourceManifestDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "defaultRole": null,
            "roles": {
                "viewer": { "inherits": [], "permissions": ["yard.read"] }
            }
        }),
    )
    .await;
}

pub(super) async fn create_invitation(
    session: &SessionFixture,
    tracker: &mut FixtureExecutionTracker,
) -> (String, String) {
    let created =
        create_http_invitation_response(session, "  Guest@Example.COM\u{2003}", &[]).await;
    assert_eq!(created["data"]["invitation"]["email"], "guest@example.com");
    assert_eq!(
        created["data"]["invitation"]["appRoles"],
        serde_json::json!([])
    );
    tracker.record_case(
        "guest-create-allows-empty-application-roles",
        &serde_json::json!({
            "operation": "createYardGuestInvite",
            "applicationRoles": []
        }),
        &serde_json::json!({"created": true, "applicationRoles": []}),
    );
    invitation_details(&created)
}

pub(super) async fn create_http_invitation(
    session: &SessionFixture,
    email: &str,
    app_roles: &[&str],
) -> (String, String) {
    let created = create_http_invitation_response(session, email, app_roles).await;
    invitation_details(&created)
}

pub(super) async fn create_foreign_invitation(session: &SessionFixture) -> String {
    let started = super::journey_tests::publish_named(
        &session.fixture,
        "deploy-foreign-invitation-001",
        "foreign-invitation",
        b"foreign index",
    )
    .await;
    let yard_id = started["data"]["yardId"].as_str().expect("foreign Yard ID");
    let created =
        create_http_invitation_response_for_yard(session, yard_id, "foreign@example.com", &[])
            .await;
    invitation_details(&created).1
}

async fn create_http_invitation_response(
    session: &SessionFixture,
    email: &str,
    app_roles: &[&str],
) -> serde_json::Value {
    create_http_invitation_response_for_yard(session, &session.yard_id, email, app_roles).await
}

async fn create_http_invitation_response_for_yard(
    session: &SessionFixture,
    yard_id: &str,
    email: &str,
    app_roles: &[&str],
) -> serde_json::Value {
    response_json(post_invitation(session, yard_id, email, app_roles).await).await
}

async fn post_invitation(
    session: &SessionFixture,
    yard_id: &str,
    email: &str,
    app_roles: &[&str],
) -> Response {
    let expires_at = crate::transfer_grants::format_expiry(
        crate::transfer_grants::now_ms().expect("current time") + 86_400_000,
    )
    .expect("expiry");
    let request = serde_json::to_vec(&serde_json::json!({
        "yardId": yard_id,
        "environmentId": null,
        "email": email,
        "appRoles": app_roles,
        "expiresAt": expires_at,
    }))
    .expect("create request");
    send(
        &session.fixture,
        "POST",
        "/v1/yards/guest-invites",
        &request,
        false,
    )
    .await
}

fn invitation_details(created: &serde_json::Value) -> (String, String) {
    let invitation_id = created["data"]["invitation"]["id"]
        .as_str()
        .expect("invitation ID")
        .to_owned();
    let invitation_url = url::Url::parse(
        created["data"]["invitationUrl"]
            .as_str()
            .expect("invitation URL"),
    )
    .expect("parsed invitation URL");
    (invitation_id, path_and_query(&invitation_url))
}

pub(super) fn create_expired_invitation(session: &SessionFixture) -> String {
    let expires_at = crate::transfer_grants::format_expiry(
        1 + blobyard_contract::YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
    )
    .expect("expired invitation expiry");
    let created = super::super::guest_invites::create(
        &session.fixture.state,
        &Principal(session.fixture.principal.clone()),
        &CreateYardGuestInviteRequest {
            yard_id: session.yard_id.clone(),
            environment_id: None,
            email: "expired@example.com".to_owned(),
            app_roles: Vec::new(),
            expires_at: Some(expires_at),
        },
        Ok(1),
    )
    .expect("expired invitation fixture");
    let json = serde_json::to_value(created.0).expect("serialized expired invitation");
    invitation_details(&json).1
}

pub(super) async fn assert_management_role_outcomes(
    session: &SessionFixture,
    tracker: &mut FixtureExecutionTracker,
) {
    let approved =
        create_http_invitation_response(session, "viewer@example.com", &["viewer"]).await;
    assert_eq!(
        approved["data"]["invitation"]["appRoles"],
        serde_json::json!(["viewer"])
    );
    tracker.record_case(
        "guest-create-accepts-approved-application-roles",
        &serde_json::json!({
            "operation": "createYardGuestInvite",
            "applicationRoles": ["viewer"],
            "policyRoles": ["viewer"]
        }),
        &serde_json::json!({"created": true, "applicationRoles": ["viewer"]}),
    );
    let invitation_count = management_invitation_count(session).await;
    assert_rejected_roles(
        session,
        "unknown@example.com",
        &["unknown"],
        "guest-create-rejects-unknown-application-roles",
        invitation_count,
        tracker,
    )
    .await;
    assert_rejected_roles(
        session,
        "duplicate@example.com",
        &["viewer", "viewer"],
        "guest-create-rejects-duplicate-application-roles",
        invitation_count,
        tracker,
    )
    .await;
}

async fn assert_rejected_roles(
    session: &SessionFixture,
    email: &str,
    app_roles: &[&str],
    case_id: &str,
    invitation_count: usize,
    tracker: &mut FixtureExecutionTracker,
) {
    let rejected = post_invitation(session, &session.yard_id, email, app_roles).await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert_eq!(management_invitation_count(session).await, invitation_count);
    tracker.record_case(
        case_id,
        &serde_json::json!({
            "operation": "createYardGuestInvite",
            "applicationRoles": app_roles,
            "policyRoles": ["viewer"]
        }),
        &serde_json::json!({"responseCode": "BAD_REQUEST", "authorityCreated": false}),
    );
}

async fn management_invitation_count(session: &SessionFixture) -> usize {
    guest_invitation_items(session).await.len()
}

pub(super) async fn guest_invitation_items(session: &SessionFixture) -> Vec<serde_json::Value> {
    let listed = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/guest-invites?yardId={}", session.yard_id),
            b"",
            false,
        )
        .await,
    )
    .await;
    listed["data"]["items"]
        .as_array()
        .expect("invitation items")
        .clone()
}
