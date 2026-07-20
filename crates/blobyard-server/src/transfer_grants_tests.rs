#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    authorize_project_binding, authorize_reservation, conceal_capability_error, format_expiry,
    idempotency_key, now_ms_from, reservation_input, reserve_or_replay, resolve_project,
    resolve_project_slugs, stable_capability, stable_upload_id, transfer_url, validate_field,
    workspace_by_id,
};
use crate::{api::AppState, repository_fault_tests::FaultingRepository};
use axum::{
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use blobyard_api_client::RequestUploadRequest;
use blobyard_contract::{
    LocalApiTokenRecord, MetadataRepository, NewUploadReservation, ProjectRecord, RepositoryError,
    WorkspaceRecord,
};
use blobyard_core::{SecretString, Slug};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

#[test]
fn generated_capability_and_clock_helpers_fail_closed_for_invalid_inputs() {
    assert!(now_ms_from(UNIX_EPOCH - Duration::from_millis(1)).is_err());
    assert_eq!(now_ms_from(UNIX_EPOCH).expect("epoch"), 0);
    let capability = SecretString::new("capability").expect("capability");
    assert_eq!(
        transfer_url("https://example.com", "transfers/uploads", &capability)
            .expect("URL")
            .expose_secret(),
        "https://example.com/transfers/uploads/capability"
    );
    assert!(transfer_url("invalid\norigin", "transfers/uploads", &capability).is_err());
}

#[test]
fn expiry_formatter_covers_valid_and_out_of_range_timestamps() {
    assert_eq!(format_expiry(0).expect("epoch"), "1970-01-01T00:00:00Z");
    assert!(format_expiry(u64::MAX).is_err());
}

#[test]
fn validation_and_capability_concealment_cover_every_stable_class() {
    assert!(validate_field("safe").is_ok());
    for value in ["", &"x".repeat(513), "line\nbreak"] {
        assert!(validate_field(value).is_err());
    }
    for error in [
        RepositoryError::NotFound,
        RepositoryError::Conflict,
        RepositoryError::InvalidInput,
    ] {
        assert_eq!(
            conceal_capability_error(error).into_response().status(),
            StatusCode::NOT_FOUND
        );
    }
    for error in [RepositoryError::SchemaTooNew, RepositoryError::Unavailable] {
        assert_eq!(
            conceal_capability_error(error).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("fixture slug")
}

fn state() -> (TempDir, AppState, ProjectRecord) {
    let temporary = TempDir::new().expect("temporary");
    let repository = Arc::new(
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository"),
    );
    let workspace = WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: slug("fixture"),
    };
    repository.create_workspace(&workspace).expect("workspace");
    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: workspace.id.clone(),
        name: "Project".to_owned(),
        slug: slug("project"),
    };
    repository.create_project(&project).expect("project");
    let storage =
        Arc::new(FilesystemStorage::open(&temporary.path().join("objects")).expect("storage"));
    let staging_directory = temporary.path().join("staging");
    std::fs::create_dir(&staging_directory).expect("staging");
    let state = AppState {
        repository,
        storage,
        capability_key: Arc::new(SecretString::new("capability-key").expect("secret")),
        public_origin: "http://127.0.0.1:8787".to_owned(),
        web_yard_origin: "http://localhost:8787".to_owned(),
        staging_directory,
        default_workspace: workspace,
        oidc_verifier: Arc::new(crate::oidc::UnavailableGithubOidcVerifier),
    };
    (temporary, state, project)
}

fn request() -> RequestUploadRequest {
    RequestUploadRequest {
        workspace: slug("fixture"),
        project: slug("project"),
        path: "builds/fixture.bin".to_owned(),
        filename: "fixture.bin".to_owned(),
        size_bytes: 4,
        checksum_sha256: "00".repeat(32),
        content_type: "application/octet-stream".to_owned(),
        git_repository: Some("example/core-project".to_owned()),
        git_commit: Some("0123456789abcdef".to_owned()),
        git_branch: Some("main".to_owned()),
    }
}

fn upload_input(state: &AppState, project: &ProjectRecord) -> NewUploadReservation {
    let capability = stable_capability(state, "principal", "request");
    reservation_input(
        &request(),
        project,
        "upload_fixture",
        &capability,
        100,
        blobyard_contract::ObjectSource::Cli,
    )
}

#[test]
fn identifiers_are_stable() {
    let (_temporary, state, _project) = state();
    assert_eq!(
        stable_upload_id("principal", "request"),
        stable_upload_id("principal", "request")
    );
    assert_ne!(
        stable_upload_id("principal", "request"),
        stable_upload_id("principal", "other")
    );
    assert_eq!(
        stable_capability(&state, "principal", "request").expose_secret(),
        stable_capability(&state, "principal", "request").expose_secret()
    );
}

#[test]
fn idempotency_headers_are_stable() {
    let mut headers = HeaderMap::new();
    assert!(idempotency_key(&headers).is_err());
    headers.insert("idempotency-key", HeaderValue::from_static("fixture"));
    assert_eq!(idempotency_key(&headers).expect("key"), "fixture");
    headers.insert("idempotency-key", HeaderValue::from_static(""));
    assert!(idempotency_key(&headers).is_err());
    headers.insert(
        "idempotency-key",
        HeaderValue::from_str(&"x".repeat(129)).expect("long header"),
    );
    assert!(idempotency_key(&headers).is_err());
    headers.insert(
        "idempotency-key",
        HeaderValue::from_bytes(&[0xff]).expect("opaque header"),
    );
    assert!(idempotency_key(&headers).is_err());
}

#[test]
fn project_resolution_and_binding_are_stable() {
    let (_temporary, state, project) = state();
    assert_eq!(
        resolve_project(&state, "workspace_fixture", &request()).expect("project"),
        project
    );
    assert!(resolve_project(&state, "other", &request()).is_err());
    assert!(
        resolve_project_slugs(
            &state,
            "workspace_fixture",
            &slug("missing"),
            &slug("project")
        )
        .is_err()
    );
    assert_eq!(
        workspace_by_id(&state, "workspace_fixture")
            .expect("workspace")
            .slug,
        slug("fixture")
    );
    assert!(workspace_by_id(&state, "missing").is_err());

    let unbound = LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash: "a".repeat(64),
        scopes: vec!["object:write".to_owned()],
        workspace_id: project.workspace_id.clone(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 100,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    assert!(authorize_project_binding(&unbound, &project).is_ok());
    let exact = LocalApiTokenRecord {
        project_id: Some(project.id.clone()),
        ..unbound.clone()
    };
    assert!(authorize_project_binding(&exact, &project).is_ok());
    let foreign = LocalApiTokenRecord {
        project_id: Some("project_foreign".to_owned()),
        ..unbound
    };
    assert_eq!(
        authorize_project_binding(&foreign, &project)
            .expect_err("foreign project")
            .into_response()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn reservation_replay_renews_expiry_and_rejects_drift() {
    let (_temporary, state, project) = state();
    let input = upload_input(&state, &project);
    let first = reserve_or_replay(&state, &input, 0, 100).expect("reservation");
    assert_eq!(first.id, "upload_fixture");
    assert_eq!(first.version.source, blobyard_contract::ObjectSource::Cli);
    assert_eq!(first.version.git_repository, input.git_repository);
    assert_eq!(first.version.git_commit, input.git_commit);
    assert_eq!(first.version.git_branch, input.git_branch);
    assert_eq!(
        authorize_reservation(&state, "workspace_fixture", &first).expect("authorized"),
        project
    );
    assert!(authorize_reservation(&state, "missing", &first).is_err());
    assert_eq!(
        reserve_or_replay(&state, &input, 0, 100).expect("stable replay"),
        first
    );

    let renewed = reserve_or_replay(&state, &input, 100, 200).expect("renewed replay");
    assert_eq!(renewed.expires_at_ms, 200);

    assert_replay_drift_rejected(&state, &input);

    let mut missing = input;
    missing.id = "missing-parent".to_owned();
    missing.project_id = "missing".to_owned();
    assert!(reserve_or_replay(&state, &missing, 0, 100).is_err());
}

fn assert_replay_drift_rejected(state: &AppState, input: &NewUploadReservation) {
    let mut drift = input.clone();
    drift.filename = "other.bin".to_owned();
    assert!(reserve_or_replay(state, &drift, 0, 200).is_err());

    let mut drift = input.clone();
    drift.source = blobyard_contract::ObjectSource::Ci;
    assert!(reserve_or_replay(state, &drift, 0, 200).is_err());

    let mut drift = input.clone();
    drift.git_repository = Some("example/other".to_owned());
    assert!(reserve_or_replay(state, &drift, 0, 200).is_err());

    let mut drift = input.clone();
    drift.git_commit = Some("fedcba9876543210".to_owned());
    assert!(reserve_or_replay(state, &drift, 0, 200).is_err());

    let mut drift = input.clone();
    drift.git_branch = Some("release".to_owned());
    assert!(reserve_or_replay(state, &drift, 0, 200).is_err());
}

#[test]
fn reservation_replay_maps_each_repository_read_and_renewal_failure() {
    for failure_index in 1..=3 {
        let (_temporary, mut state, project) = state();
        let input = upload_input(&state, &project);
        reserve_or_replay(&state, &input, 0, 100).expect("initial reservation");
        let repository = Arc::clone(&state.repository);
        state.repository = Arc::new(FaultingRepository::new(repository, failure_index));

        let response = reserve_or_replay(&state, &input, 100, 200)
            .expect_err("repository failure")
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
