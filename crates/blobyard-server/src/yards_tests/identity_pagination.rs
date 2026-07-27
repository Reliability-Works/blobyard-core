#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{mutate, session_support::setup};

#[tokio::test]
async fn management_role_mutation_finds_an_existing_assignment_on_the_second_page() {
    let session = setup().await;
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
    let target_user_id = session
        .fixture
        .seed_yard_management_role_page(&session.yard_id);
    let updated = mutate(
        &session.fixture,
        "/v1/yards/management-roles/set",
        serde_json::json!({
            "yardId": session.yard_id,
            "userId": target_user_id,
            "role": "developer",
        }),
    )
    .await;
    assert_eq!(updated["data"]["role"], "developer");
    assert_eq!(updated["data"]["userId"], target_user_id);
}
