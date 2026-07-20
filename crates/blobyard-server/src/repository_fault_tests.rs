#![allow(clippy::expect_used, reason = "test synchronization must fail loudly")]

use crate::Repository;
use blobyard_contract::{LifecycleRepository, MetadataRepository, RepositoryError};
use std::sync::Arc;

#[path = "repository_fault_ci.rs"]
mod ci;
#[path = "repository_fault_credentials.rs"]
mod credentials;
#[path = "repository_fault_inboxes.rs"]
mod inboxes;
#[path = "repository_fault_previews.rs"]
mod previews;
#[path = "repository_fault_sharing.rs"]
mod sharing;
#[path = "repository_fault_transfers.rs"]
mod transfers;
#[path = "repository_fault_yards.rs"]
mod yards;

#[derive(Clone, Copy)]
pub(crate) enum Corruption {
    CompletedVersion,
    CompletedPath,
    CompletedSize,
    CompletedChecksum,
    AbortedStorageKey,
    ShareObjectSize,
    ShareExpiry,
    InboxExpiry,
    PreviewCreatedAt,
    PreviewExpiresAt,
}

pub(crate) struct FaultingRepository {
    inner: Arc<dyn Repository>,
    failures: blobyard_testkit::FailureCounter,
    corruption: Option<Corruption>,
}

impl FaultingRepository {
    pub(crate) const fn new(inner: Arc<dyn Repository>, failure_index: usize) -> Self {
        Self {
            inner,
            failures: blobyard_testkit::FailureCounter::new(failure_index),
            corruption: None,
        }
    }

    pub(crate) const fn corrupting(inner: Arc<dyn Repository>, corruption: Corruption) -> Self {
        Self {
            inner,
            failures: blobyard_testkit::FailureCounter::new(usize::MAX),
            corruption: Some(corruption),
        }
    }

    fn check(&self) -> Result<(), RepositoryError> {
        self.failures.check()
    }
}

fn conforming_repository() -> (tempfile::TempDir, Arc<dyn Repository>) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository = blobyard_repository_sqlite::SqliteRepository::open(
        &temporary.path().join("metadata.sqlite3"),
    )
    .expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    blobyard_testkit::transfer_conformance(&repository, "project_fixture")
        .expect("transfer conformance");
    (temporary, Arc::new(repository))
}

impl MetadataRepository for FaultingRepository {
    blobyard_testkit::impl_faulting_metadata_repository!();
}

impl LifecycleRepository for FaultingRepository {
    blobyard_testkit::impl_faulting_lifecycle_repository!();
}

#[test]
fn faulting_repository_forwards_before_its_exact_failure_index() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let inner: Arc<dyn Repository> = Arc::new(
        blobyard_repository_sqlite::SqliteRepository::open(
            &temporary.path().join("metadata.sqlite3"),
        )
        .expect("repository"),
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).schema_version(),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(FaultingRepository::new(inner, 1).schema_version(), Ok(16));
}

#[test]
fn faulting_repository_forwards_workspace_renames() {
    let (_temporary, inner) = conforming_repository();
    let renamed = blobyard_contract::WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Forwarded workspace".to_owned(),
        slug: blobyard_core::Slug::new("forwarded").expect("forwarded slug"),
    };
    let mut event = blobyard_testkit::workspace_renamed_event(&renamed.id, "renamed", 3);
    event.id = "audit_forwarded_rename".to_owned();
    event.request_id = "request_forwarded_rename".to_owned();
    let repository = FaultingRepository::new(Arc::clone(&inner), usize::MAX);

    assert_eq!(repository.rename_workspace(&renamed, &event), Ok(()));
    assert_eq!(inner.workspace_by_slug(&renamed.slug), Ok(renamed));
}

#[test]
fn faulting_repository_forwards_the_remaining_lifecycle_operations() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let inner: Arc<dyn Repository> = Arc::new(
        blobyard_repository_sqlite::SqliteRepository::open(
            &temporary.path().join("metadata.sqlite3"),
        )
        .expect("repository"),
    );
    let repository = FaultingRepository::new(Arc::clone(&inner), usize::MAX);

    assert_eq!(repository.retained_projects(), Ok(Vec::new()));
    assert_eq!(
        repository.fail_retention("missing", 1),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).retained_projects(),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(inner, 0).fail_retention("missing", 1),
        Err(RepositoryError::Unavailable)
    );
}

#[path = "repository_fault_tests/workflow_tests.rs"]
mod workflow;

#[path = "repository_fault_tests/ci_tests.rs"]
mod ci_tests;

#[path = "repository_fault_tests/transfer_tests.rs"]
mod transfer_tests;

#[path = "repository_fault_tests/share_tests.rs"]
mod share_tests;

#[path = "repository_fault_tests/inbox_tests.rs"]
mod inbox_tests;

#[path = "repository_fault_tests/preview_tests.rs"]
mod preview_tests;

#[path = "repository_fault_tests/yard_tests.rs"]
mod yard_tests;
