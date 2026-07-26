//! Complete wire encoders for local-user request models.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    CreateLocalUserRequest, DeactivateLocalUserRequest, ListLocalUsersQuery,
    ResetLocalUserLoginKeyRequest,
};
use blobyard_core::Slug;

fn workspace() -> Slug {
    Slug::new("team".to_owned()).expect("workspace")
}

#[test]
fn local_user_requests_encode_exactly_without_secret_material() {
    assert_eq!(
        ListLocalUsersQuery {
            workspace: workspace()
        }
        .into_query(),
        "workspace=team"
    );
    assert_eq!(
        CreateLocalUserRequest {
            workspace: workspace(),
            display_name: "Ada Lovelace".to_owned(),
            email: Some("ada@example.test".to_owned()),
        }
        .into_json(),
        serde_json::json!({
            "displayName": "Ada Lovelace",
            "email": "ada@example.test",
            "workspace": "team",
        })
    );
    assert_eq!(
        CreateLocalUserRequest {
            workspace: workspace(),
            display_name: "No email".to_owned(),
            email: None,
        }
        .into_json(),
        serde_json::json!({
            "displayName": "No email",
            "workspace": "team",
        })
    );
    assert_eq!(
        ResetLocalUserLoginKeyRequest {
            user_id: "user_1".to_owned()
        }
        .into_json(),
        serde_json::json!({ "userId": "user_1" })
    );
    assert_eq!(
        DeactivateLocalUserRequest {
            user_id: "user_1".to_owned()
        }
        .into_json(),
        serde_json::json!({ "userId": "user_1" })
    );
}
