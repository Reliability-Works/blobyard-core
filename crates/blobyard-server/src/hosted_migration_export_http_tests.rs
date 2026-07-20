#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::http_fixture::{Fixture, options, ready_export, token};
use super::*;
use axum::http::StatusCode;
use serde_json::{Value, json};

fn export_part(bytes: &[u8], dataset: &str) -> ExportPart {
    ExportPart {
        byte_size: bytes.len() as u64,
        checksum_sha256: checksum(bytes),
        dataset: dataset.to_owned(),
        part_number: 1,
    }
}

#[tokio::test]
async fn export_request_states_fail_closed() {
    let (fixture, server) = Fixture::spawn().await;
    let api = api_client(&fixture.origin).expect("API");
    for request in [
        json!({ "exportId": "", "status": "queued" }),
        json!({ "exportId": "export-fixture", "status": "ready" }),
    ] {
        *fixture.request.lock().expect("request") = request;
        assert_eq!(
            request_export(&api, &token()).await.err(),
            Some(HostedMigrationError::InvalidExport)
        );
    }
    *fixture.request.lock().expect("request") =
        json!({ "exportId": "export-fixture", "status": "running" });
    assert!(request_export(&api, &token()).await.is_ok());
    server.abort();
}

#[tokio::test]
async fn export_poll_states_fail_closed() {
    let (fixture, server) = Fixture::spawn().await;
    let api = api_client(&fixture.origin).expect("API");
    let migration_options = options(fixture.origin.clone());
    for (case, (state, expected)) in [
        (Value::Null, HostedMigrationError::SourceApi),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "wrong", "status": "ready" }),
            HostedMigrationError::InvalidExport,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": "failed", "id": "export-fixture", "status": "ready" }),
            HostedMigrationError::InvalidExport,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "failed" }),
            HostedMigrationError::SourceApi,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "expired" }),
            HostedMigrationError::SourceApi,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "unknown" }),
            HostedMigrationError::InvalidExport,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "queued" }),
            HostedMigrationError::SourceApi,
        ),
        (
            json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "running" }),
            HostedMigrationError::SourceApi,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        *fixture.export.lock().expect("export") = state;
        assert_eq!(
            wait_for_export(&api, &token(), &migration_options, "export-fixture")
                .await
                .err(),
            Some(expected),
            "export state case {case}"
        );
    }
    *fixture.export.lock().expect("export") = ready_export();
    assert!(
        wait_for_export(&api, &token(), &migration_options, "export-fixture")
            .await
            .is_ok()
    );
    server.abort();
}

#[tokio::test]
async fn artifact_byte_fetch_enforces_bounds_and_transport_integrity() {
    let (fixture, server) = Fixture::spawn().await;
    let fetch = fetch_client().expect("fetch");
    let secret_url = fixture.payload_url();

    fixture.set_payload(StatusCode::INTERNAL_SERVER_ERROR, b"error", None, false);
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 10).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    fixture.set_payload(StatusCode::OK, b"a", Some(11), false);
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 10).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    fixture.set_payload(StatusCode::OK, b"abc", None, true);
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 2).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 3).await.expect("bytes"),
        b"abc"
    );
    let invalid_url = SecretString::new("not-a-url").expect("invalid URL");
    assert_eq!(
        fetch_bytes(&fetch, &invalid_url, 3).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    fixture.set_payload(StatusCode::OK, b"abc", Some(4), true);
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 10).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    fixture.set_stream_error(b"a", None);
    assert_eq!(
        fetch_bytes(&fetch, &secret_url, 10).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );
    server.abort();
}

#[tokio::test]
async fn artifact_datasets_enforce_shape_and_source_availability() {
    let (fixture, server) = Fixture::spawn().await;
    let api = api_client(&fixture.origin).expect("API");
    let fetch = fetch_client().expect("fetch");
    let bytes = serde_json::to_vec(&json!({ "dataset": "wrong", "records": [] })).expect("dataset");
    fixture.set_payload(StatusCode::OK, &bytes, None, false);
    let expected = export_part(&bytes, "workspace");
    assert_eq!(
        download_datasets(
            &api,
            &fetch,
            &token(),
            "export-fixture",
            std::slice::from_ref(&expected),
        )
        .await
        .err(),
        Some(HostedMigrationError::InvalidExport)
    );
    assert_eq!(
        download_datasets(&api, &fetch, &token(), "export-fixture", &[])
            .await
            .err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let malformed = b"not-json";
    fixture.set_payload(StatusCode::OK, malformed, None, false);
    let malformed_part = export_part(malformed, "workspace");
    assert_eq!(
        download_datasets(&api, &fetch, &token(), "export-fixture", &[malformed_part],)
            .await
            .err(),
        Some(HostedMigrationError::InvalidExport)
    );
    fixture.set_payload(StatusCode::INTERNAL_SERVER_ERROR, b"error", None, false);
    assert_eq!(
        download_datasets(
            &api,
            &fetch,
            &token(),
            "export-fixture",
            std::slice::from_ref(&expected),
        )
        .await
        .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    assert_eq!(
        download_artifact(&api, &fetch, &token(), "export-fixture", 1, None)
            .await
            .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    server.abort();
}

#[tokio::test]
async fn artifact_download_rejects_checksum_mismatch() {
    let (fixture, server) = Fixture::spawn().await;
    let api = api_client(&fixture.origin).expect("API");
    let fetch = fetch_client().expect("fetch");
    fixture.set_payload(StatusCode::OK, b"abc", None, false);
    let mismatched = ExportPart {
        byte_size: 3,
        checksum_sha256: "f".repeat(64),
        dataset: "workspace".to_owned(),
        part_number: 1,
    };
    assert_eq!(
        download_artifact(
            &api,
            &fetch,
            &token(),
            "export-fixture",
            1,
            Some(&mismatched)
        )
        .await
        .err(),
        Some(HostedMigrationError::Integrity)
    );
    server.abort();
}

#[test]
fn client_configuration_rejects_invalid_source_origins() {
    assert!(fetch_client().is_ok());
    assert!(matches!(
        api_client("not an origin"),
        Err(HostedMigrationError::InvalidInput)
    ));
    let _fault = test_faults::activate(test_faults::ExportFault::ApiTransport);
    assert_eq!(
        api_client("http://127.0.0.1:8787").err(),
        Some(HostedMigrationError::SourceApi)
    );
}

#[tokio::test]
async fn download_orchestration_preserves_each_stage_failure() {
    let mut migration_options = options("not an origin".to_owned());
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::InvalidInput)
    );

    let (fixture, server) = Fixture::spawn().await;
    migration_options = options(fixture.origin.clone());
    {
        let _fault = test_faults::activate(test_faults::ExportFault::FetchClient);
        assert_eq!(
            download(&migration_options, token()).await.err(),
            Some(HostedMigrationError::SourceDownload)
        );
    }

    *fixture.request.lock().expect("request") =
        json!({ "exportId": "export-fixture", "status": "ready" });
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::InvalidExport)
    );
    *fixture.request.lock().expect("request") =
        json!({ "exportId": "export-fixture", "status": "queued" });

    *fixture.export.lock().expect("export") = json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "failed" });
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::SourceApi)
    );

    *fixture.export.lock().expect("export") =
        json!({ "artifactCount": 1, "errorCode": null, "id": "export-fixture", "status": "ready" });
    fixture.set_payload(StatusCode::INTERNAL_SERVER_ERROR, b"error", None, false);
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::SourceDownload)
    );

    fixture.set_payload(StatusCode::OK, b"not-json", None, false);
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let empty_index = serde_json::to_vec(&json!({
        "dataset": "complete",
        "records": [{ "format": "Blob Yard account export v1", "parts": [] }]
    }))
    .expect("index");
    fixture.set_payload(StatusCode::OK, &empty_index, None, false);
    assert_eq!(
        download(&migration_options, token()).await.err(),
        Some(HostedMigrationError::InvalidExport)
    );
    server.abort();
}
