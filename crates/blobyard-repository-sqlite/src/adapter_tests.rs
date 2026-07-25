#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{SqliteRepository, ci_test_fixtures as ci_fixtures, configure};
use blobyard_contract::{
    CiRepository, CredentialRepository, InboxRepository, LifecycleRepository, LocalUserRepository,
    MachineSessionMintResult, MetadataRepository, NewAuditEvent, NewDownloadGrant, NewInbox,
    NewInboxUpload, NewObjectDeletion, NewObjectVersion, NewPreview, NewPreviewFile, NewShare,
    NewUploadReservation, ObjectDeletionTarget, PreviewRepository, ProjectRecord, RepositoryError,
    RetentionPolicyRecord, SharingRepository, TransferRepository, WebYardRepository,
    WorkspaceRecord, YardSessionRepository,
};
use blobyard_core::Slug;
use rusqlite::{
    Connection,
    hooks::{AuthAction, Authorization},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[path = "adapter_group_principal_tests.rs"]
mod group_principal_tests;
#[path = "adapter_group_row_tests.rs"]
mod group_row_tests;
#[path = "adapter_group_validation_tests.rs"]
mod group_validation_tests;
#[path = "adapter_token_fixtures.rs"]
mod token_fixtures;

use token_fixtures::{session, token, token_audit};

fn yard_fixture() -> blobyard_testkit::YardConformanceFixture {
    blobyard_testkit::YardConformanceFixture::new("docs", "inactive", "history")
        .expect("Yard conformance fixture")
}

pub(super) fn install_denial(connection: &Connection, denied_index: usize) -> Arc<AtomicUsize> {
    let observed = Arc::new(AtomicUsize::new(0));
    let callback_observed = Arc::clone(&observed);
    connection
        .authorizer(Some(move |context: rusqlite::hooks::AuthContext<'_>| {
            if matches!(
                context.action,
                AuthAction::Read { .. } | AuthAction::Function { .. } | AuthAction::Recursive
            ) {
                return Authorization::Allow;
            }
            let index = callback_observed.fetch_add(1, Ordering::Relaxed);
            if index == denied_index {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }))
        .expect("authorizer");
    observed
}

fn assert_tables(repository: &SqliteRepository, tables: &[&str]) {
    let connection = repository.test_connection().expect("connection");
    for table in tables {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .expect("table query");
        assert!(exists, "{table}");
    }
    drop(connection);
}

fn run_contract(repository: &SqliteRepository) -> Result<(), RepositoryError> {
    blobyard_testkit::repository_conformance(repository)?;
    let workspace = repository
        .list_workspaces()?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    blobyard_testkit::credential_conformance(repository, &workspace.id)?;
    blobyard_testkit::local_user_conformance(repository, &workspace.id)?;
    blobyard_testkit::group_conformance(repository, &workspace.id)?;
    run_ci_contract(repository)?;
    blobyard_testkit::transfer_conformance(repository, "project_fixture")?;
    blobyard_testkit::sharing_conformance(repository)?;
    blobyard_testkit::inbox_conformance(repository)?;
    blobyard_testkit::preview_conformance(repository)?;
    blobyard_testkit::yard_conformance(repository, &yard_fixture())?;
    blobyard_testkit::lifecycle_conformance(repository)
}

fn run_ci_contract(repository: &SqliteRepository) -> Result<(), RepositoryError> {
    let trust = ci_fixtures::trust("trust_contract", None, 10_000);
    repository.create_ci_trust(
        &trust,
        &ci_fixtures::event("ci.trust_created", "ci_trust", &trust.id, 10_000),
    )?;
    let _listed = repository.list_ci_trusts(&trust.workspace_id)?;
    let mut session = ci_fixtures::session(10_001, 10_001);
    session.workspace = Some("renamed".to_owned());
    let minted = repository.mint_machine_session(
        &session,
        &ci_fixtures::event("ci.token_minted", "project", "project_fixture", 10_001),
    )?;
    if !matches!(minted, MachineSessionMintResult::Minted(_)) {
        return Err(RepositoryError::Unavailable);
    }
    let _authenticated = repository.authenticate_machine_session(&session.id, 10_002)?;
    if !repository.revoke_ci_trust(
        &trust.id,
        &trust.workspace_id,
        10_003,
        &ci_fixtures::event("ci.trust_revoked", "ci_trust", &trust.id, 10_003),
    )? {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn denied_contract(denied_index: usize) -> (Result<(), RepositoryError>, usize) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    let observed = {
        let connection = repository.connection.lock().expect("connection");
        install_denial(&connection, denied_index)
    };
    let result = run_contract(&repository);
    (result, observed.load(Ordering::Relaxed))
}

fn denied_initialization(denied_index: usize) -> (Result<(), RepositoryError>, usize) {
    let mut connection = Connection::open_in_memory().expect("connection");
    let observed = install_denial(&connection, denied_index);
    let result = configure(&connection).and_then(|()| super::migrations::apply(&mut connection));
    (result, observed.load(Ordering::Relaxed))
}

fn assert_denial_sweep(
    operation: impl Fn(usize) -> (Result<(), RepositoryError>, usize),
) -> Option<usize> {
    for denied_index in 0..4_000 {
        let (result, observed) = operation(denied_index);
        if observed <= denied_index {
            result.expect("operation succeeds after every authorization point");
            return Some(denied_index);
        }
        assert_eq!(result, Err(RepositoryError::Unavailable));
    }
    None
}

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("slug")
}

fn checksum(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn workspace() -> WorkspaceRecord {
    WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: slug("fixture"),
    }
}

fn project() -> ProjectRecord {
    ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: slug("project"),
    }
}

fn version() -> NewObjectVersion {
    NewObjectVersion {
        id: "version_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: "fixture.bin".to_owned(),
        version: 1,
        storage_key: "objects/version_fixture".to_owned(),
        source: blobyard_contract::ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}

fn upload() -> NewUploadReservation {
    NewUploadReservation {
        id: "upload_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: "fixture.bin".to_owned(),
        filename: "fixture.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size: 1,
        expected_checksum: checksum('a'),
        storage_key: "objects/upload_fixture".to_owned(),
        capability_hash: checksum('b'),
        expires_at_ms: 2,
        created_at_ms: 1,
        source: blobyard_contract::ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    }
}

fn audit() -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "fixture.recorded".to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "fixture".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    }
}

fn deletion() -> NewObjectDeletion {
    NewObjectDeletion {
        id: "delete_fixture".to_owned(),
        target: ObjectDeletionTarget {
            project_id: "project_fixture".to_owned(),
            object_path: "fixture.bin".to_owned(),
            version: Some(1),
        },
        actor: "fixture".to_owned(),
        request_id: "request_fixture".to_owned(),
        created_at_ms: 1,
    }
}

fn policy() -> RetentionPolicyRecord {
    RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: None,
        branch_glob: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

pub(super) fn empty_repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&database).expect("repository");
    (temporary, repository)
}

pub(super) fn group_repository() -> (tempfile::TempDir, SqliteRepository) {
    let (temporary, repository) = empty_repository();
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let user = blobyard_testkit::local_user("workspace_fixture", "user_group", None, 30);
    repository
        .create_local_user(
            &user,
            &blobyard_testkit::login_key("userkey_group", &user.id, 'a', 30),
            &blobyard_testkit::local_user_event("audit_user_group", &user, "user.created", 30),
        )
        .expect("group user");
    (temporary, repository)
}

fn unavailable<T>(result: Result<T, RepositoryError>) {
    assert_eq!(result.err(), Some(RepositoryError::Unavailable));
}

fn invalid<T>(result: Result<T, RepositoryError>) {
    assert_eq!(result.err(), Some(RepositoryError::InvalidInput));
}

#[path = "adapter_failure_map_tests.rs"]
mod failure_mapping;

#[path = "adapter_invalid_tests.rs"]
mod invalid_inputs;

#[path = "inventory_tests.rs"]
mod inventory;

#[path = "adapter_state_tests.rs"]
mod state_failures;

#[path = "adapter_behavior_tests.rs"]
mod stable_behavior;

#[path = "adapter_transfer_behavior_tests.rs"]
mod transfer_behavior;

#[path = "adapter_token_tests.rs"]
mod token_behavior;

#[path = "adapter_local_user_tests.rs"]
mod local_user_behavior;

#[path = "adapter_group_tests.rs"]
mod group_behavior;

#[path = "adapter_group_edge_tests.rs"]
mod group_edges;

#[path = "adapter_group_fault_tests.rs"]
mod group_faults;

#[path = "adapter_group_limit_tests.rs"]
mod group_limits;

#[path = "adapter_workspace_tests.rs"]
mod workspace_behavior;

#[path = "adapter_migration_tests.rs"]
mod migration_behavior;

#[path = "adapter_application_migration_tests.rs"]
mod application_migration_behavior;

#[path = "adapter_multipart_tests.rs"]
mod multipart_behavior;
