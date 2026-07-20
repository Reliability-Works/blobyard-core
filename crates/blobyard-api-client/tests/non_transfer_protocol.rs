//! Complete wire encoders for every non-transfer request model.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    ClearRetentionResponse, CreatePreviewRequest, CreateWorkspaceRequest, CursorQuery,
    GitHubOidcExchangeRequest, ListPreviewsQuery, ListSharesQuery, ResolveInboxQuery,
    ResolvePreviewQuery, ResolveShareQuery, RetentionPolicy, RetentionQuery, RevokePreviewRequest,
    RevokeShareRequest, SetRetentionRequest, ShareDownloadRequest,
};
use blobyard_core::{SecretString, Slug};
use std::num::NonZeroU32;

#[test]
fn resource_and_identity_requests_encode_exactly() {
    assert_eq!(
        CursorQuery {
            cursor: Some("next page".into())
        }
        .into_query(),
        "cursor=next+page"
    );
    assert_eq!(CursorQuery::default().into_query(), "");
    assert_eq!(
        CreateWorkspaceRequest {
            name: "Release builds".into()
        }
        .into_json(),
        serde_json::json!({ "name": "Release builds" })
    );
    let exchange = GitHubOidcExchangeRequest {
        assertion: secret("oidc-assertion"),
        actions: vec!["upload".into(), "share".into()],
        project: "mobile".into(),
        workspace: Some("release".into()),
    };
    let exchange = exchange.into_request();
    assert_eq!(
        exchange.body(),
        Some(&serde_json::json!({
            "actions": ["upload", "share"],
            "project": "mobile",
            "workspace": "release",
        }))
    );
    assert_eq!(
        exchange.bearer().map(SecretString::expose_secret),
        Some("oidc-assertion")
    );
    assert!(
        exchange
            .body()
            .is_none_or(|body| body.get("assertion").is_none())
    );

    let default_workspace = GitHubOidcExchangeRequest {
        assertion: secret("oidc-default"),
        actions: vec!["download".into()],
        project: "mobile".into(),
        workspace: None,
    }
    .into_request();
    assert_eq!(
        default_workspace.body(),
        Some(&serde_json::json!({ "actions": ["download"], "project": "mobile" }))
    );
}

#[test]
fn capability_requests_encode_exactly() {
    assert_eq!(
        ResolveShareQuery {
            token: secret("share token")
        }
        .into_query(),
        "token=share+token"
    );
    assert_eq!(
        ShareDownloadRequest {
            token: secret("share-token")
        }
        .into_json(),
        serde_json::json!({ "token": "share-token" })
    );
    assert_eq!(
        RevokeShareRequest {
            share_id: "share_1".into()
        }
        .into_json(),
        serde_json::json!({ "shareId": "share_1" })
    );
    assert_eq!(
        ListSharesQuery {
            workspace: Slug::new("team").expect("workspace")
        }
        .into_query(),
        "workspace=team"
    );
    assert_eq!(
        ResolveInboxQuery {
            token: secret("inbox token")
        }
        .into_query(),
        "token=inbox+token"
    );
}

#[test]
fn preview_requests_encode_exactly() {
    let request = CreatePreviewRequest {
        workspace: Slug::new("team").expect("workspace"),
        project: Slug::new("web").expect("project"),
        manifest_id: "manifest_1".into(),
        expires: Some("7d".into()),
    };
    assert_eq!(
        request.into_json(),
        serde_json::json!({
            "workspace": "team",
            "project": "web",
            "manifestId": "manifest_1",
            "expires": "7d"
        })
    );
    assert_eq!(
        ResolvePreviewQuery {
            token: secret("preview token"),
            path: "assets/app.js".into()
        }
        .into_query(),
        "token=preview+token&path=assets%2Fapp.js"
    );
    assert_eq!(
        ListPreviewsQuery {
            workspace: Slug::new("team").expect("workspace"),
            project: Slug::new("web").expect("project")
        }
        .into_query(),
        "workspace=team&project=web"
    );
    assert_eq!(
        RevokePreviewRequest {
            preview_id: "preview_1".into()
        }
        .into_json(),
        serde_json::json!({ "previewId": "preview_1" })
    );
}

#[test]
fn retention_requests_encode_exactly() {
    assert_eq!(
        RetentionQuery {
            workspace: Slug::new("team").expect("workspace"),
            project: Slug::new("app").expect("project"),
        }
        .into_query(),
        "workspace=team&project=app"
    );
    assert_eq!(
        SetRetentionRequest {
            workspace: Slug::new("team").expect("workspace"),
            project: Slug::new("app").expect("project"),
            policy: RetentionPolicy {
                keep_latest: NonZeroU32::new(3).expect("positive retention count"),
                branch_glob: Some("main".into()),
                path_glob: Some("builds/**".into()),
            },
        }
        .into_json(),
        serde_json::json!({
            "workspace": "team",
            "project": "app",
            "keepLatest": 3,
            "branch": "main",
            "path": "builds/**",
        })
    );
    assert_eq!(
        SetRetentionRequest {
            workspace: Slug::new("team").expect("workspace"),
            project: Slug::new("app").expect("project"),
            policy: RetentionPolicy {
                keep_latest: NonZeroU32::new(1).expect("positive retention count"),
                branch_glob: None,
                path_glob: None,
            },
        }
        .into_json(),
        serde_json::json!({
            "workspace": "team",
            "project": "app",
            "keepLatest": 1,
        })
    );
}

#[test]
fn clear_retention_response_decodes_the_api_contract() {
    assert_eq!(
        serde_json::from_value::<ClearRetentionResponse>(serde_json::json!({ "cleared": true }))
            .expect("clear retention response"),
        ClearRetentionResponse { cleared: true }
    );
}

fn secret(value: &str) -> SecretString {
    SecretString::new(value).expect("fixture secret")
}
