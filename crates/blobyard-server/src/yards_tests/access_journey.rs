use super::{
    access_edge_tests::seed_reader, body, host, journey_tests::publish, mutate, public_request,
};
use crate::{
    contract_test_support::{assert_error, response_json, send},
    transfers::test_seams,
};
use axum::http::StatusCode;

async fn read_access(fixture: &test_seams::TransferFixture, yard_id: &str) -> serde_json::Value {
    response_json(
        send(
            fixture,
            "GET",
            &format!("/v1/yards/access?yardId={yard_id}"),
            b"",
            false,
        )
        .await,
    )
    .await
}

async fn assert_concealed_like_unknown_host(
    fixture: &test_seams::TransferFixture,
    concealed_host: &str,
) {
    for path in ["/", "/missing.txt"] {
        assert_concealed_get(fixture, concealed_host, path).await;
        assert_concealed_head(fixture, concealed_host, path).await;
    }
}

async fn assert_concealed_get(
    fixture: &test_seams::TransferFixture,
    concealed_host: &str,
    path: &str,
) {
    let concealed = public_request(fixture, "GET", path, concealed_host, None).await;
    let unknown = public_request(fixture, "GET", path, "unknown-123456789-fixture", None).await;
    assert_eq!(concealed.status(), unknown.status(), "GET {path}");
    assert_error(concealed, StatusCode::NOT_FOUND, "NOT_FOUND").await;
    assert_error(unknown, StatusCode::NOT_FOUND, "NOT_FOUND").await;
}

async fn assert_concealed_head(
    fixture: &test_seams::TransferFixture,
    concealed_host: &str,
    path: &str,
) {
    let concealed = public_request(fixture, "HEAD", path, concealed_host, None).await;
    let unknown = public_request(fixture, "HEAD", path, "unknown-123456789-fixture", None).await;
    assert_eq!(concealed.status(), StatusCode::NOT_FOUND, "HEAD {path}");
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    assert_eq!(body(concealed).await, body(unknown).await);
}

#[tokio::test]
async fn visibility_journey_conceals_non_public_yards_until_restored() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let first = publish(&fixture, "deploy-access-001", b"first index").await;
    let stable_host = host(&first, "url");
    let first_host = host(&first, "deploymentUrl");
    let yard_id = first["data"]["yardId"].as_str().expect("yard ID");
    let access = read_access(&fixture, yard_id).await;
    assert_eq!(access["data"]["visibility"], "public");
    assert_eq!(access["data"]["grants"], serde_json::json!([]));
    let served = public_request(&fixture, "GET", "/", &stable_host, None).await;
    assert_eq!(served.status(), StatusCode::OK);
    let hidden = mutate(
        &fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": yard_id, "visibility": "owner" }),
    )
    .await;
    assert_eq!(hidden["data"]["visibility"], "owner");
    assert_eq!(
        read_access(&fixture, yard_id).await["data"]["visibility"],
        "owner"
    );
    assert_concealed_like_unknown_host(&fixture, &stable_host).await;
    assert_concealed_like_unknown_host(&fixture, &first_host).await;
    let restored = mutate(
        &fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": yard_id, "visibility": "public" }),
    )
    .await;
    assert_eq!(restored["data"]["visibility"], "public");
    for public_host in [stable_host.as_str(), first_host.as_str()] {
        let index = public_request(&fixture, "GET", "/", public_host, None).await;
        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(body(index).await.as_ref(), b"first index");
        let missing = public_request(&fixture, "GET", "/missing.txt", public_host, None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(body(missing).await.as_ref(), b"not found");
    }
}

#[tokio::test]
async fn grant_lifecycle_lists_scopes_and_revokes_idempotently() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    seed_reader(&fixture);
    let first = publish(&fixture, "deploy-access-001", b"first index").await;
    let yard_id = first["data"]["yardId"].as_str().expect("yard ID");
    approve_access_roles(&fixture, yard_id).await;
    let open = mutate(
        &fixture,
        "/v1/yards/access/grant",
        serde_json::json!({
            "yardId": yard_id,
            "principalKind": "user",
            "principalId": "user_reader",
            "appRoles": ["editor", "viewer"]
        }),
    )
    .await;
    assert_eq!(open["data"]["grant"]["principalKind"], "user");
    assert_eq!(open["data"]["grant"]["principalId"], "user_reader");
    assert_eq!(
        open["data"]["grant"]["appRoles"],
        serde_json::json!(["editor", "viewer"])
    );
    assert_eq!(
        open["data"]["grant"]["environmentId"],
        serde_json::Value::Null
    );
    assert_eq!(open["data"]["grant"]["expiresAt"], serde_json::Value::Null);
    let scoped = mutate(
        &fixture,
        "/v1/yards/access/grant",
        serde_json::json!({
            "yardId": yard_id,
            "principalKind": "guest-invite",
            "principalId": "invite_reviewer",
            "appRoles": [],
            "environmentId": format!("yardenv_{yard_id}"),
            "expiresAt": "2100-01-01T00:00:00Z"
        }),
    )
    .await;
    assert_eq!(
        scoped["data"]["grant"]["environmentId"],
        format!("yardenv_{yard_id}")
    );
    assert_eq!(scoped["data"]["grant"]["expiresAt"], "2100-01-01T00:00:00Z");
    let listed = read_access(&fixture, yard_id).await;
    let grants = listed["data"]["grants"].as_array().expect("grants");
    assert_eq!(grants.len(), 2);
    let open_id = open["data"]["grant"]["id"].as_str().expect("grant ID");
    assert_revocation(&fixture, yard_id, open_id, &scoped["data"]["grant"]["id"]).await;
}

async fn approve_access_roles(fixture: &test_seams::TransferFixture, yard_id: &str) {
    mutate(
        fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": yard_id,
            "userId": "user_reader",
            "role": "owner"
        }),
    )
    .await;
    mutate(
        fixture,
        "/v1/yards/application-policy",
        serde_json::json!({
            "yardId": yard_id,
            "sourceManifestDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "defaultRole": null,
            "roles": {
                "editor": { "inherits": ["viewer"], "permissions": ["yard.write"] },
                "viewer": { "inherits": [], "permissions": ["yard.read"] }
            }
        }),
    )
    .await;
}

async fn assert_revocation(
    fixture: &test_seams::TransferFixture,
    yard_id: &str,
    open_id: &str,
    scoped_id: &serde_json::Value,
) {
    mutate(
        fixture,
        "/v1/yards/access/revoke",
        serde_json::json!({ "yardId": yard_id, "grantId": open_id }),
    )
    .await;
    let repeated = mutate(
        fixture,
        "/v1/yards/access/revoke",
        serde_json::json!({ "yardId": yard_id, "grantId": open_id }),
    )
    .await;
    assert_eq!(repeated["data"], serde_json::json!({}));
    let remaining = read_access(fixture, yard_id).await;
    let remaining_grants = remaining["data"]["grants"].as_array().expect("grants");
    assert_eq!(remaining_grants.len(), 1);
    assert_eq!(&remaining_grants[0]["id"], scoped_id);
    assert_error(
        send(
            fixture,
            "POST",
            "/v1/yards/access/revoke",
            &serde_json::to_vec(
                &serde_json::json!({ "yardId": yard_id, "grantId": "grant_missing" }),
            )
            .expect("revoke request"),
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(
            fixture,
            "GET",
            "/v1/yards/access?yardId=yard_missing",
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}
