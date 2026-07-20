#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::discovery::UploadFile;
use super::file_facts::FileFacts;
use super::http::SignedTransferClient;
use super::provenance::GitProvenance;
use super::resume::ResumeState;
use blobyard_core::Slug;
use std::path::PathBuf;

fn fixture() -> crate::runner::login::tests::support::Fixture {
    crate::runner::login::tests::support::Fixture::new(&["blobyard", "whoami"], Vec::new())
}

fn upload_file(source: PathBuf) -> UploadFile {
    UploadFile {
        source,
        logical_path: "artifact.bin".into(),
        filename: "artifact.bin".into(),
    }
}

#[tokio::test]
async fn upload_file_maps_inspection_failure() {
    let fixture = fixture();
    let temp = tempfile::tempdir().expect("temp");
    let file = upload_file(temp.path().to_path_buf());
    let result = fixture
        .runner
        .upload_file(
            &SignedTransferClient::new(),
            &Slug::new("team").expect("workspace"),
            &Slug::new("app").expect("project"),
            &file,
            &GitProvenance::default(),
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn resumed_upload_rejects_excessive_part_counts() {
    let fixture = fixture();
    let temp = tempfile::tempdir().expect("temp");
    let file = upload_file(temp.path().join("unused"));
    let part_size = 8 * 1024 * 1024;
    let facts = FileFacts {
        size_bytes: part_size * 10_001,
        checksum_sha256: "a".repeat(64),
        content_type: "application/octet-stream".into(),
        fingerprint: "b".repeat(64),
    };
    let state = ResumeState::new("upload_large".into(), facts.fingerprint.clone(), part_size);
    let result = fixture
        .runner
        .multipart_upload(
            &SignedTransferClient::new(),
            &file,
            &facts,
            &temp.path().join("resume.json"),
            state,
            &indicatif::ProgressBar::hidden(),
        )
        .await;
    assert!(result.is_err());
}
