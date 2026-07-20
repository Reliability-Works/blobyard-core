#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::auth::hash;
use crate::{Repository, api, api::AppState, error::ApiError};
use axum::{Router, body::Body, response::IntoResponse, response::Response};
use blobyard_api_client::RequestUploadRequest;
use blobyard_contract::{LocalApiTokenRecord, ProjectRecord, RepositoryError, WorkspaceRecord};
use blobyard_core::{SecretString, Slug};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use std::sync::Arc;
use tempfile::TempDir;

/// Isolated transfer router whose durable state lives as long as the fixture.
pub struct TransferFixture {
    router: Router,
    pub(crate) state: AppState,
    pub(crate) principal: LocalApiTokenRecord,
    pub(crate) project: ProjectRecord,
    repository: Arc<SqliteRepository>,
    root: TempDir,
}

/// Deterministic pre-mutation failures in upload-grant issuance.
#[derive(Clone, Copy)]
pub enum IssueFailure {
    /// The system clock cannot provide a timestamp.
    Clock,
    /// The current timestamp cannot be extended by the grant lifetime.
    ExpiryOverflow,
    /// The configured transfer origin cannot form a secret-bearing URL.
    TransferUrl,
}

impl TransferFixture {
    /// Returns a clone of the isolated router.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Removes the isolated retention overview table to force a provider failure.
    pub fn break_retention_overview(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("DROP TABLE retention_runs")
            .expect("remove retention overview table");
    }

    /// Removes the isolated audit table to force a provider failure after workspace resolution.
    pub fn break_audit_listing(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("DROP TABLE audit_events")
            .expect("remove audit table");
    }

    /// Removes CI trust storage to force a trust provider failure.
    pub fn break_ci_trusts(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("DROP TABLE ci_trusts")
            .expect("remove CI trust table");
    }

    /// Removes machine-session storage after trust lookup to force revoke failure.
    pub fn break_ci_revoke(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("DROP TABLE machine_sessions")
            .expect("remove machine session table");
    }

    /// Removes multipart-part storage to force upload status provider failure.
    pub fn break_upload_parts(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("DROP TABLE upload_parts")
            .expect("remove upload part table");
    }

    /// Renames workspace storage to force the first trust lookup to fail.
    pub fn break_workspace_listing(&self) {
        self.repository
            .test_connection()
            .expect("repository connection")
            .execute_batch("ALTER TABLE workspaces RENAME TO unavailable_workspaces")
            .expect("rename workspace table");
    }

    /// Corrupts one machine token's project binding without changing its trusted session.
    pub fn corrupt_machine_project(&self, raw_token: &str) {
        let changed = self
            .repository
            .test_connection()
            .expect("repository connection")
            .execute(
                "UPDATE api_tokens SET project_id = 'project_other' WHERE secret_hash = ?1",
                [hash(raw_token)],
            )
            .expect("corrupt machine project");
        assert_eq!(changed, 1);
    }
}

#[path = "transfers_fixture_seams.rs"]
mod fixture_behaviors;

/// Exercises response formatting with a timestamp outside the supported RFC 3339 range.
#[must_use]
pub fn expiry_format_failure() -> Response {
    super::upload_response(
        "upload_fixture".to_owned(),
        SecretString::new("https://example.com/upload").expect("URL"),
        blobyard_contract::ReservationStrategy::Single,
        None,
        u64::MAX,
    )
    .err()
    .expect("expiry failure")
    .into_response()
}

/// Builds an authenticated transfer router with exactly the requested scopes.
#[must_use]
pub fn fixture(scopes: &[&str]) -> TransferFixture {
    let root = TempDir::new().expect("temporary fixture");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging directory");
    let repository = Arc::new(
        SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository"),
    );
    let state = fixture_state(&root, staging, Arc::clone(&repository));
    let project = fixture_project(&state);
    state
        .repository
        .create_workspace(&state.default_workspace)
        .expect("workspace");
    state.repository.create_project(&project).expect("project");
    let principal = fixture_principal(&state, scopes);
    install_principal(&state, &principal);
    let router = fixture_router(&state);
    TransferFixture {
        router,
        state,
        principal,
        project,
        repository,
        root,
    }
}

fn fixture_state(
    root: &TempDir,
    staging_directory: std::path::PathBuf,
    sqlite_repository: Arc<SqliteRepository>,
) -> AppState {
    let repository: Arc<dyn Repository> = sqlite_repository;
    let storage =
        Arc::new(FilesystemStorage::open(&root.path().join("objects")).expect("storage fixture"));
    AppState {
        repository,
        storage,
        capability_key: Arc::new(SecretString::new("capability").expect("secret")),
        public_origin: "http://127.0.0.1:8787".to_owned(),
        web_yard_origin: "http://localhost:8787".to_owned(),
        staging_directory,
        default_workspace: WorkspaceRecord {
            id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: Slug::new("fixture").expect("slug"),
        },
        oidc_verifier: Arc::new(crate::oidc::UnavailableGithubOidcVerifier),
    }
}

fn fixture_project(state: &AppState) -> ProjectRecord {
    ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: state.default_workspace.id.clone(),
        name: "Project".to_owned(),
        slug: Slug::new("project").expect("slug"),
    }
}

fn fixture_principal(state: &AppState, scopes: &[&str]) -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash: hash("secret"),
        scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        workspace_id: state.default_workspace.id.clone(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: i64::MAX as u64,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn install_principal(state: &AppState, principal: &LocalApiTokenRecord) {
    state
        .repository
        .install_bootstrap(&hash("bootstrap"))
        .expect("bootstrap");
    state
        .repository
        .exchange_bootstrap(
            &hash("bootstrap"),
            principal,
            &blobyard_contract::LocalCliSessionRecord {
                id: "session_fixture".to_owned(),
                token_id: principal.id.clone(),
                workspace_id: principal.workspace_id.clone(),
                name: principal.name.clone(),
                platform: "test".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                created_at_ms: principal.created_at_ms,
                last_used_at_ms: None,
                revoked_at_ms: None,
            },
        )
        .expect("access token");
}

pub(crate) fn fixture_router(state: &AppState) -> Router {
    api::router_with_state(state.clone())
}

fn upload_request() -> RequestUploadRequest {
    RequestUploadRequest {
        workspace: Slug::new("fixture").expect("workspace slug"),
        project: Slug::new("project").expect("project slug"),
        path: "object.bin".to_owned(),
        filename: "object.bin".to_owned(),
        size_bytes: 1,
        checksum_sha256: "00".repeat(32),
        content_type: "application/octet-stream".to_owned(),
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}
