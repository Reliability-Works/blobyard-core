#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::http_fixture::{Fixture, download_grant, options, token};
use super::*;
use axum::http::StatusCode;
use std::collections::BTreeMap;

fn source_object() -> SourceObject {
    SourceObject {
        version_id: "version-fixture".to_owned(),
        uri: "blobyard://source/project/file.txt?version=1".to_owned(),
        size: 3,
        checksum: checksum(b"abc"),
    }
}

struct ObjectFetchFixture {
    fixture: Fixture,
    server: tokio::task::JoinHandle<Result<(), std::io::Error>>,
    fetch: reqwest::Client,
    secret_url: SecretString,
    root: tempfile::TempDir,
    path: std::path::PathBuf,
    object: SourceObject,
}

impl ObjectFetchFixture {
    async fn spawn() -> Self {
        let (fixture, server) = Fixture::spawn().await;
        let root = tempfile::tempdir().expect("paths");
        let path = root.path().join("object");
        let secret_url = fixture.payload_url();
        Self {
            fixture,
            server,
            fetch: fetch_client().expect("fetch"),
            secret_url,
            root,
            path,
            object: source_object(),
        }
    }

    async fn assert_fetch_error(&self, expected: HostedMigrationError) {
        assert_eq!(
            fetch_object(&self.fetch, &self.secret_url, &self.object, &self.path)
                .await
                .err(),
            Some(expected)
        );
    }
}

#[tokio::test]
async fn object_fetch_rejects_transport_size_and_checksum_failures() {
    let context = ObjectFetchFixture::spawn().await;

    context
        .fixture
        .set_payload(StatusCode::INTERNAL_SERVER_ERROR, b"error", None, false);
    context
        .assert_fetch_error(HostedMigrationError::SourceDownload)
        .await;
    context
        .fixture
        .set_payload(StatusCode::OK, b"abc", Some(4), false);
    context
        .assert_fetch_error(HostedMigrationError::SourceDownload)
        .await;
    context
        .fixture
        .set_payload(StatusCode::OK, b"abcd", None, true);
    context
        .assert_fetch_error(HostedMigrationError::Integrity)
        .await;
    context
        .fixture
        .set_payload(StatusCode::OK, b"abd", None, true);
    context
        .assert_fetch_error(HostedMigrationError::Integrity)
        .await;
    context.server.abort();
}

#[tokio::test]
async fn object_fetch_rejects_unsafe_urls_paths_and_streams() {
    let context = ObjectFetchFixture::spawn().await;
    let invalid_url = SecretString::new("not-a-url").expect("invalid URL");
    assert_eq!(
        fetch_object(&context.fetch, &invalid_url, &context.object, &context.path)
            .await
            .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    context
        .fixture
        .set_payload(StatusCode::OK, b"abc", None, false);
    let missing_parent = context.root.path().join("missing").join("object");
    assert_eq!(
        fetch_object(
            &context.fetch,
            &context.secret_url,
            &context.object,
            &missing_parent,
        )
        .await
        .err(),
        Some(HostedMigrationError::Persistence)
    );
    let mut truncated = context.object.clone();
    truncated.size = 4;
    context
        .fixture
        .set_payload(StatusCode::OK, b"abc", Some(4), true);
    assert_eq!(
        fetch_object(
            &context.fetch,
            &context.secret_url,
            &truncated,
            &context.path,
        )
        .await
        .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    context.fixture.set_stream_error(b"a", None);
    assert_eq!(
        fetch_object(
            &context.fetch,
            &context.secret_url,
            &context.object,
            &context.path,
        )
        .await
        .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    context.server.abort();
}

#[tokio::test]
async fn object_downloads_reject_unsafe_grants_and_duplicate_identity() {
    let (fixture, server) = Fixture::spawn().await;
    let fetch = fetch_client().expect("fetch");
    let object = source_object();
    let source = DownloadedExport {
        datasets: BTreeMap::new(),
        api: api_client(&fixture.origin).expect("API"),
        fetch,
        token: token(),
    };
    let mut invalid = object.clone();
    invalid.uri = "not-a-blobyard-uri".to_owned();
    assert_eq!(
        download_objects(&source, &[invalid]).await.err(),
        Some(HostedMigrationError::InvalidExport)
    );

    *fixture.download.lock().expect("download") =
        download_grant(&fixture.origin, 4, &object.checksum);
    assert_eq!(
        download_objects(&source, std::slice::from_ref(&object))
            .await
            .err(),
        Some(HostedMigrationError::Integrity)
    );
    *fixture.download.lock().expect("download") =
        download_grant(&fixture.origin, 3, &object.checksum);
    fixture.set_payload(StatusCode::INTERNAL_SERVER_ERROR, b"error", None, false);
    assert_eq!(
        download_objects(&source, std::slice::from_ref(&object))
            .await
            .err(),
        Some(HostedMigrationError::SourceDownload)
    );
    fixture.set_payload(StatusCode::OK, b"abc", None, false);
    assert_eq!(
        download_objects(&source, &[object.clone(), object])
            .await
            .err(),
        Some(HostedMigrationError::InvalidExport)
    );
    server.abort();
}

#[tokio::test]
async fn object_download_local_io_failures_are_preserved() {
    let context = ObjectFetchFixture::spawn().await;
    context
        .fixture
        .set_payload(StatusCode::OK, b"abc", None, false);

    for fault in [
        test_faults::ExportFault::CreateFile,
        test_faults::ExportFault::WriteFile,
        test_faults::ExportFault::FlushFile,
        test_faults::ExportFault::SyncFile,
    ] {
        let guard = test_faults::activate(fault);
        assert_eq!(
            fetch_object(
                &context.fetch,
                &context.secret_url,
                &context.object,
                &context.path,
            )
            .await
            .err(),
            Some(HostedMigrationError::Persistence),
            "fault {fault:?}"
        );
        drop(guard);
    }

    let source = DownloadedExport {
        datasets: BTreeMap::new(),
        api: api_client(&context.fixture.origin).expect("API"),
        fetch: context.fetch.clone(),
        token: token(),
    };
    let _fault = test_faults::activate(test_faults::ExportFault::TemporaryDirectory);
    assert_eq!(
        download_objects(&source, &[]).await.err(),
        Some(HostedMigrationError::Persistence)
    );
    context.server.abort();
}

#[tokio::test]
async fn unavailable_source_api_is_redacted_at_each_export_boundary() {
    let api = api_client("http://127.0.0.1:1").expect("API");
    let migration_options = options("http://127.0.0.1:1".to_owned());
    assert_eq!(
        request_export(&api, &token()).await.err(),
        Some(HostedMigrationError::SourceApi)
    );
    assert_eq!(
        wait_for_export(&api, &token(), &migration_options, "export")
            .await
            .err(),
        Some(HostedMigrationError::SourceApi)
    );
    assert_eq!(
        download_artifact(
            &api,
            &fetch_client().expect("fetch"),
            &token(),
            "export",
            0,
            None,
        )
        .await
        .err(),
        Some(HostedMigrationError::SourceApi)
    );

    let source = DownloadedExport {
        datasets: BTreeMap::new(),
        api,
        fetch: fetch_client().expect("fetch"),
        token: token(),
    };
    assert_eq!(
        download_objects(&source, &[source_object()]).await.err(),
        Some(HostedMigrationError::SourceApi)
    );
}
