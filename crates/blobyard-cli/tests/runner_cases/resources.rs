//! Project and object command behavior over the typed API seam.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

#[tokio::test]
async fn lists_and_creates_projects_with_typed_scope() {
    let list = Fixture::new(
        &["blobyard", "projects", "list", "--workspace", "team"],
        vec![ok(
            serde_json::json!({
                "items": [{
                    "id": "project_1",
                    "workspaceSlug": "team",
                    "slug": "mobile",
                    "name": "Mobile"
                }],
                "nextCursor": "cursor_2"
            }),
            "req_projects",
        )],
        Some("ci-token"),
        None,
    );
    let result = list
        .runner
        .execute(&list.command)
        .await
        .expect("list projects");
    assert_eq!(result_json(result)["data"]["items"][0]["slug"], "mobile");
    let requests = list.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::ListProjects);
    assert_eq!(requests[0].query(), Some("workspace=team"));

    let create = Fixture::new(
        &[
            "blobyard",
            "projects",
            "create",
            "Mobile Builds",
            "--workspace",
            "team",
        ],
        vec![ok(
            serde_json::json!({
                "id": "project_2",
                "workspaceSlug": "team",
                "slug": "mobile-builds",
                "name": "Mobile Builds"
            }),
            "req_create",
        )],
        Some("ci-token"),
        None,
    );
    let result = create
        .runner
        .execute(&create.command)
        .await
        .expect("create project");
    assert_eq!(result_json(result)["data"]["slug"], "mobile-builds");
    let requests = create.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CreateProject);
    assert_eq!(requests[0].idempotency_key(), None);
    assert_eq!(
        requests[0].body().and_then(|body| body.get("name")),
        Some(&serde_json::json!("Mobile Builds"))
    );
}

#[tokio::test]
async fn handles_empty_projects_and_rejects_missing_scope_or_bad_names() {
    let empty = Fixture::new(
        &["blobyard", "projects", "list", "--workspace", "team"],
        vec![ok(
            serde_json::json!({ "items": [], "nextCursor": null }),
            "req_empty",
        )],
        Some("ci-token"),
        None,
    );
    empty
        .runner
        .execute(&empty.command)
        .await
        .expect("empty list");

    let missing = Fixture::new(
        &["blobyard", "projects", "list"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        missing
            .runner
            .execute(&missing.command)
            .await
            .expect_err("workspace required")
            .code(),
        ErrorCode::InvalidRequest
    );

    for name in ["", "line\nbreak", &"x".repeat(129)] {
        let fixture = Fixture::new(
            &[
                "blobyard",
                "projects",
                "create",
                name,
                "--workspace",
                "team",
            ],
            Vec::new(),
            Some("ci-token"),
            None,
        );
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid name")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}

#[tokio::test]
async fn project_creation_requires_workspace_scope() {
    let fixture = Fixture::new(
        &["blobyard", "projects", "create", "Mobile Builds"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("workspace required")
            .code(),
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn lists_objects_from_configured_scope_and_uri_prefix() {
    let scoped = Fixture::new(
        &[
            "blobyard",
            "ls",
            "--workspace",
            "team",
            "--project",
            "mobile",
            "--versions",
        ],
        vec![ok(
            serde_json::json!({
                "items": [{
                    "uri": "blobyard://team/mobile/builds/app.zip?version=2",
                    "filename": "app.zip",
                    "sizeBytes": 42,
                    "createdAt": "2026-07-09T00:00:00Z",
                    "availability": "available",
                    "source": "cli"
                }],
                "nextCursor": null
            }),
            "req_objects",
        )],
        Some("ci-token"),
        None,
    );
    let result = scoped
        .runner
        .execute(&scoped.command)
        .await
        .expect("list objects");
    assert_eq!(result_json(result)["data"]["items"][0]["sizeBytes"], 42);
    let requests = scoped.transport.requests();
    assert!(
        requests[0]
            .query()
            .is_some_and(|query| query.contains("versions=true"))
    );

    let prefixed = Fixture::new(
        &["blobyard", "ls", "blobyard://other/project/builds/main"],
        vec![ok(
            serde_json::json!({ "items": [], "nextCursor": null }),
            "req_prefix",
        )],
        Some("ci-token"),
        None,
    );
    prefixed
        .runner
        .execute(&prefixed.command)
        .await
        .expect("prefix list");
    let requests = prefixed.transport.requests();
    let query = requests[0].query().expect("query");
    assert!(query.contains("workspace=other"));
    assert!(query.contains("project=project"));
    assert!(query.contains("prefix=builds%2Fmain"));
}

#[tokio::test]
async fn object_listing_validates_scope_and_uri() {
    let missing_workspace = Fixture::new(&["blobyard", "ls"], Vec::new(), Some("ci-token"), None);
    assert_eq!(
        missing_workspace
            .runner
            .execute(&missing_workspace.command)
            .await
            .expect_err("workspace required")
            .code(),
        ErrorCode::InvalidRequest
    );
    let missing_project = Fixture::new(
        &["blobyard", "ls", "--workspace", "team"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        missing_project
            .runner
            .execute(&missing_project.command)
            .await
            .expect_err("project required")
            .code(),
        ErrorCode::InvalidRequest
    );
    let invalid = Fixture::new(
        &["blobyard", "ls", "not-a-uri"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        invalid
            .runner
            .execute(&invalid.command)
            .await
            .expect_err("invalid uri")
            .code(),
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn object_removal_validates_uri() {
    let remove = Fixture::new(
        &["blobyard", "rm", "blobyard://team/mobile/old.zip"],
        vec![ok(
            serde_json::json!({
                "uri": "blobyard://team/mobile/old.zip",
                "deleted": true
            }),
            "req_delete",
        )],
        Some("ci-token"),
        None,
    );
    remove
        .runner
        .execute(&remove.command)
        .await
        .expect("remove");
    let requests = remove.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::DeleteObject);
    assert_eq!(requests[0].idempotency_key(), None);

    let invalid_remove = Fixture::new(
        &["blobyard", "rm", "invalid"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        invalid_remove
            .runner
            .execute(&invalid_remove.command)
            .await
            .expect_err("invalid uri")
            .code(),
        ErrorCode::InvalidRequest
    );
}
