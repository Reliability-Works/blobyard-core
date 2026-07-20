use crate::lifecycle::{create_project, list_objects, store_version};
use crate::lifecycle_support::{
    corrupt_object_as_directory, project_id, remove_corruption, storage_records,
};
use crate::support::{authorized_server, send};
use axum::http::StatusCode;
use blobyard_contract::{LifecycleRepository, RepositoryError};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_server::enforce_retention;
use serde_json::json;

#[tokio::test]
async fn object_delete_reports_partial_storage_failure_and_retries_durable_plan() {
    let server = authorized_server().await;
    create_project(&server).await;
    store_version(&server, "retry/object.txt", "delete-retry-one").await;
    store_version(&server, "retry/object.txt", "delete-retry-two").await;
    let records = storage_records(server.temporary.path(), "retry/object.txt");
    assert_eq!(records.len(), 2);
    let failing_key = &records[1].1;
    corrupt_object_as_directory(server.temporary.path(), failing_key);

    let body = json!({ "uri": "blobyard://default/documentation/retry/object.txt" });
    let failed = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(body.clone()),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(failed.1["error"]["code"], "INTERNAL_ERROR");
    let still_listed = list_objects(&server.router, &server.access_token, true).await;
    assert_eq!(still_listed.0, StatusCode::OK);
    assert_eq!(
        still_listed.1["data"]["items"]
            .as_array()
            .expect("items")
            .len(),
        2
    );

    remove_corruption(server.temporary.path(), failing_key);
    let retried = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(body),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(retried.0, StatusCode::OK);
    assert_eq!(retried.1["data"]["deleted"], true);
    let replayed = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": "blobyard://default/documentation/retry/object.txt" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(replayed.0, StatusCode::OK);
    let empty = list_objects(&server.router, &server.access_token, true).await;
    assert_eq!(empty.1["data"]["items"], json!([]));

    let next = store_version(&server, "retry/object.txt", "delete-retry-three").await;
    assert!(next.ends_with("version=3"));
}

#[tokio::test]
async fn retention_failure_is_recorded_and_the_same_plan_resumes() {
    let server = authorized_server().await;
    create_project(&server).await;
    for key in [
        "retention-retry-one",
        "retention-retry-two",
        "retention-retry-three",
    ] {
        store_version(&server, "retention/retry.txt", key).await;
    }
    let set = send(
        &server.router,
        "PUT",
        "/v1/retention",
        Some(json!({
            "workspace": "default", "project": "documentation",
            "keepLatest": 1, "path": "retention/**"
        })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(set.0, StatusCode::OK);

    let repository = SqliteRepository::open(&server.temporary.path().join("metadata.sqlite3"))
        .expect("repository");
    let plan = repository
        .begin_retention(
            &project_id(server.temporary.path()),
            "retention_retry",
            "system:retention",
            "request_retention_retry",
            1,
        )
        .expect("retention plan");
    assert_eq!(plan.items.len(), 2);
    let failing_key = &plan.items[1].storage_key;
    corrupt_object_as_directory(server.temporary.path(), failing_key);

    assert_eq!(
        enforce_retention(server.temporary.path()),
        Err(blobyard_server::ServerError::Storage)
    );
    let failed = repository
        .retention_overview(&project_id(server.temporary.path()))
        .expect("failed overview")
        .last_run
        .expect("failed run");
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.deleted_count, 0);
    assert_eq!(
        repository.fail_retention("missing_retention", 2),
        Err(RepositoryError::NotFound)
    );

    remove_corruption(server.temporary.path(), failing_key);
    enforce_retention(server.temporary.path()).expect("resumed retention");
    let complete = repository
        .retention_overview(&project_id(server.temporary.path()))
        .expect("complete overview")
        .last_run
        .expect("complete run");
    assert_eq!(complete.id, plan.id);
    assert_eq!(complete.status, "complete");
    assert_eq!(complete.deleted_count, 2);
    let list = list_objects(&server.router, &server.access_token, true).await;
    assert_eq!(list.1["data"]["items"].as_array().expect("items").len(), 1);
}
