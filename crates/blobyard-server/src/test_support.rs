#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{Repository, api::AppState, error::ApiError};
use axum::{http::StatusCode, response::IntoResponse};
use blobyard_contract::{
    AuditValue, CiAction, GithubOidcIdentity, MultipartId, NewAuditEvent, NewMachineSession,
    ObjectChecksum, ObjectStorage, ObjectVersionRecord, StorageError, StorageKey, StorageMetadata,
    StoredObjectRecord, UploadState,
};
use blobyard_core::SecretString;
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

pub(crate) fn install_machine_session(
    fixture: &crate::transfers::test_seams::TransferFixture,
    raw_token: &str,
    fixture_id: &str,
    now_ms: u64,
) {
    let repository = "reliability-works/blobyard-core";
    let trust = blobyard_testkit::ci_trust(
        &format!("trust_{fixture_id}"),
        &fixture.principal.workspace_id,
        Some(&fixture.project.id),
        &fixture.state.public_origin,
        now_ms,
    );
    fixture
        .state
        .repository
        .create_ci_trust(
            &trust,
            &machine_event(
                "ci.trust_created",
                "ci_trust",
                &trust.id,
                &trust.workspace_id,
                repository,
                now_ms,
            ),
        )
        .expect("create machine trust");
    let session = NewMachineSession {
        id: format!("machine_{fixture_id}"),
        token_prefix: format!("byd_ci_{fixture_id}"),
        secret_hash: crate::auth::hash(raw_token),
        identity: GithubOidcIdentity {
            audience: fixture.state.public_origin.clone(),
            repository: repository.to_owned(),
            git_ref: "refs/heads/main".to_owned(),
            workflow_path: ".github/workflows/release.yml".to_owned(),
            workflow_ref: "refs/heads/main".to_owned(),
            environment: None,
            run_id: fixture_id.to_owned(),
            run_attempt: Some("1".to_owned()),
            sha: Some("a".repeat(40)),
            expires_at_ms: now_ms + 600_000,
        },
        workspace: Some(fixture.state.default_workspace.slug.to_string()),
        project: fixture.project.slug.to_string(),
        actions: vec![CiAction::Upload],
        oidc_token_hash: crate::auth::hash(&format!("{fixture_id}-assertion")),
        now_ms,
    };
    let _ = fixture
        .state
        .repository
        .mint_machine_session(
            &session,
            &machine_event(
                "ci.token_minted",
                "project",
                &fixture.project.id,
                &fixture.principal.workspace_id,
                repository,
                now_ms,
            ),
        )
        .expect("mint machine session");
}

fn machine_event(
    action: &str,
    target_type: &str,
    target_id: &str,
    workspace_id: &str,
    repository: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{target_id}"),
        workspace_id: workspace_id.to_owned(),
        actor: "github:reliability-works/blobyard-core".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{target_id}"),
        target_type: target_type.to_owned(),
        metadata: vec![
            (
                "repository".to_owned(),
                AuditValue::String(repository.to_owned()),
            ),
            (
                "targetId".to_owned(),
                AuditValue::String(target_id.to_owned()),
            ),
        ],
        created_at_ms,
    }
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
    crate::transfers::test_seams::fixture_state_with_repository(
        staging_directory,
        repository,
        storage,
    )
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
