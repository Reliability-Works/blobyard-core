use super::*;

#[tokio::test]
async fn exchange_clock_failure_stops_before_verification_or_persistence() {
    let fixture = state(None);
    let error = exchange_at(
        &fixture.state,
        &headers("Bearer aaa.bbb.clock"),
        Ok(Json(request(&["upload"]))),
        Err(ApiError::internal()),
    )
    .await
    .err()
    .expect("clock failure");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn rate_limit_and_audit_are_enforced_at_the_exchange_boundary() {
    let fixture = state(None);
    for index in 1..=20 {
        let _response = exchange(
            State(fixture.state.clone()),
            headers(&format!("Bearer aaa.bbb.{index}")),
            Ok(Json(request(&["upload"]))),
        )
        .await
        .expect("exchange within rate limit");
    }
    let limited = exchange(
        State(fixture.state.clone()),
        headers("Bearer aaa.bbb.21"),
        Ok(Json(request(&["upload"]))),
    )
    .await
    .err()
    .expect("rate limited")
    .into_response();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key("retry-after"));

    let audit = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 50)
        .expect("audit");
    let minted = audit
        .items
        .iter()
        .filter(|event| event.action == "ci.token_minted")
        .collect::<Vec<_>>();
    assert_eq!(minted.len(), 20);
    assert!(minted.iter().all(|event| {
        event.actor == "github:reliability-works/blobyard-core"
            && event.target_type == "project"
            && event.metadata.iter().any(|(name, value)| {
                name == "targetId" && value == &AuditValue::String(fixture.project.id.clone())
            })
    }));
    let audit_debug = format!("{audit:?}");
    assert!(!audit_debug.contains("aaa.bbb"));
    assert!(!audit_debug.contains("byd_ci_"));
}
