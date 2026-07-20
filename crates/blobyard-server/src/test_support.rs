#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{Repository, api::AppState, error::ApiError};
use axum::{http::StatusCode, response::IntoResponse};
use blobyard_contract::{
    MultipartId, ObjectChecksum, ObjectStorage, ObjectVersionRecord, StorageError, StorageKey,
    StorageMetadata, StoredObjectRecord, UploadState, WorkspaceRecord,
};
use blobyard_core::{SecretString, Slug};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug)]
pub(crate) struct FailingReader;

impl std::io::Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("fixture read failure"))
    }
}

pub(crate) fn assert_multipart_unavailable(storage: &dyn ObjectStorage) {
    let key = StorageKey::new("fixture/multipart").expect("storage key");
    let upload = MultipartId("upload_fixture".to_owned());
    let metadata = StorageMetadata {
        size: 0,
        checksum: ObjectChecksum::new("0".repeat(64)).expect("checksum"),
    };
    assert_eq!(
        storage.begin_multipart(&key, &metadata),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        storage.put_part(&upload, 1, &mut std::io::Cursor::new([])),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        storage.complete_multipart(&upload, &[]),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        storage.abort_multipart(&upload),
        Err(StorageError::Unavailable)
    );
}

pub(crate) fn invalid_s3_configuration() -> crate::S3RuntimeConfiguration {
    crate::S3RuntimeConfiguration::new(
        "not-a-url".to_owned(),
        "region".to_owned(),
        "bucket".to_owned(),
        SecretString::new("access").expect("access key"),
        SecretString::new("secret").expect("secret key"),
        None,
    )
}

pub(crate) fn error_status<T>(result: Result<T, ApiError>) -> StatusCode {
    result
        .err()
        .expect("operation failure")
        .into_response()
        .status()
}

#[path = "test_support/multipart_storage.rs"]
pub mod multipart_storage;
#[path = "test_support/multipart_upload.rs"]
pub mod multipart_upload;

pub(crate) fn state(
    root: &TempDir,
    staging_directory: PathBuf,
    storage: Arc<dyn ObjectStorage>,
) -> AppState {
    let repository: Arc<dyn Repository> = Arc::new(
        SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository"),
    );
    let default_workspace = WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: Slug::new("fixture").expect("slug"),
    };
    AppState {
        repository,
        storage,
        capability_key: Arc::new(SecretString::new("capability").expect("secret")),
        public_origin: "http://127.0.0.1:8787".to_owned(),
        web_yard_origin: "http://localhost:8787".to_owned(),
        staging_directory,
        default_workspace,
        oidc_verifier: Arc::new(crate::oidc::UnavailableGithubOidcVerifier),
    }
}

pub(crate) fn filesystem_state(root: &TempDir, staging_directory: PathBuf) -> AppState {
    let storage =
        Arc::new(FilesystemStorage::open(&root.path().join("objects")).expect("storage fixture"));
    state(root, staging_directory, storage)
}

pub(crate) fn stored_object() -> StoredObjectRecord {
    StoredObjectRecord {
        version: ObjectVersionRecord {
            id: "version_fixture".to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "builds/app.zip".to_owned(),
            version: 1,
            storage_key: "valid/key".to_owned(),
            state: UploadState::Complete,
            size: Some(42),
            checksum: Some("00".repeat(32)),
            created_at_ms: 0,
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: Some("example/core-project".to_owned()),
            git_commit: Some("0123456789abcdef".to_owned()),
            git_branch: Some("main".to_owned()),
        },
        filename: "app.zip".to_owned(),
        content_type: "application/zip".to_owned(),
    }
}
