#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::http_fixture::{Fixture, options, token};
use super::*;

#[tokio::test]
async fn complete_export_and_object_download_cover_the_successful_internal_orchestration() {
    let (fixture, server) = Fixture::spawn().await;
    fixture.configure_complete_export();
    let source = download(&options(fixture.origin.clone()), token())
        .await
        .expect("complete export");
    assert_eq!(source.datasets.len(), REQUIRED_DATASETS.len());

    fixture.set_payload(axum::http::StatusCode::OK, b"abc", None, false);
    let object = SourceObject {
        version_id: "version-fixture".to_owned(),
        uri: "blobyard://source/project/file.txt?version=1".to_owned(),
        size: 3,
        checksum: checksum(b"abc"),
    };
    let downloaded = download_objects(&source, &[object])
        .await
        .expect("object download");
    let bytes = std::fs::read(
        downloaded
            .paths
            .get("version-fixture")
            .expect("downloaded object path"),
    )
    .expect("downloaded object bytes");
    assert_eq!(bytes, b"abc");
    server.abort();
}
