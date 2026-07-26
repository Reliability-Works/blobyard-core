use super::session_support::{
    SessionFixture, assert_identity_login_location, body, browser_request, private_get, setup,
    sign_in,
};
use crate::contract_test_support::{response_json, send};
use axum::http::{StatusCode, header};

#[tokio::test]
async fn private_yard_login_exchange_management_and_revocation_are_live() {
    let session = setup().await;
    let signed_in = sign_in(&session, "/docs/?q=one").await;
    let replay = browser_request(
        &session.fixture,
        "GET",
        &signed_in.exchange_path,
        &format!("{}:8787", session.host),
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::FOUND);
    assert_identity_login_location(replay.headers());

    let served = private_get(&session, &signed_in.cookie, false).await;
    assert_eq!(served.status(), StatusCode::OK);
    assert_eq!(body(served).await.as_ref(), b"docs index");
    let wrong_host_cookie = browser_request(
        &session.fixture,
        "GET",
        "/docs/",
        &format!("{}:8787", session.deployment_host),
        &[("accept", "text/html")],
        "",
        Some(&signed_in.cookie),
    )
    .await;
    assert_eq!(wrong_host_cookie.status(), StatusCode::FOUND);

    let mismatched_logout = browser_request(
        &session.fixture,
        "POST",
        "/.blobyard/session/logout",
        &format!("{}:8787", session.host),
        &[(header::ORIGIN.as_str(), "https://attacker.example")],
        "",
        Some(&signed_in.cookie),
    )
    .await;
    assert_eq!(mismatched_logout.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        private_get(&session, &signed_in.cookie, false)
            .await
            .status(),
        StatusCode::OK
    );

    assert_list_and_management_revoke(&session, &signed_in.cookie).await;
}

async fn assert_list_and_management_revoke(session: &SessionFixture, cookie: &str) {
    let listed = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/sessions?yardId={}", session.yard_id),
            b"",
            false,
        )
        .await,
    )
    .await;
    let sessions = listed["data"]["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["status"], "active");
    assert_eq!(sessions[0]["userId"], session.user_id);
    assert_eq!(sessions[0]["yardId"], session.yard_id);
    assert_eq!(
        sessions[0]["hostLabel"],
        session.host.strip_suffix(".localhost").expect("host label")
    );
    assert!(sessions[0]["lastUsedAt"].is_string());
    let session_id = sessions[0]["id"].as_str().expect("session ID");
    revoke_management_session(session, session_id).await;
    assert_eq!(
        private_get(session, cookie, true).await.status(),
        StatusCode::FOUND
    );
}

async fn revoke_management_session(session: &SessionFixture, session_id: &str) {
    let body = serde_json::to_vec(&serde_json::json!({
        "sessionId": session_id,
        "yardId": session.yard_id,
    }))
    .expect("revoke body");
    for _attempt in 0..2 {
        let response = send(
            &session.fixture,
            "POST",
            "/v1/yards/sessions/revoke",
            &body,
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}
