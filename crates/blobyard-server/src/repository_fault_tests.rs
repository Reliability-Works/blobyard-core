#![allow(clippy::expect_used, reason = "test synchronization must fail loudly")]

use crate::Repository;
use blobyard_contract::{
    LifecycleRepository, MetadataRepository, NewAuditEvent, NewYardContinuation, NewYardSession,
    RepositoryError, YardSessionAuditContext, YardSessionRepository,
};
use std::sync::Arc;

#[path = "repository_fault_ci.rs"]
mod ci;
#[path = "repository_fault_credentials.rs"]
mod credentials;
#[path = "repository_fault_groups.rs"]
mod groups;
#[path = "repository_fault_inboxes.rs"]
mod inboxes;
#[path = "repository_fault_local_users.rs"]
mod local_users;
#[path = "repository_fault_previews.rs"]
mod previews;
#[path = "repository_fault_sharing.rs"]
mod sharing;
#[path = "repository_fault_transfers.rs"]
mod transfers;
#[path = "repository_fault_yard_guests.rs"]
mod yard_guests;
#[path = "repository_fault_yard_identity.rs"]
mod yard_identity;
#[path = "repository_fault_yard_sessions.rs"]
mod yard_sessions;
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
    YardSessionCreatedAt,
    YardManagementRoleTimestamp,
    YardPolicyTimestamp,
    YardPolicyRevision,
    YardAccessGrantTimestamp,
    YardGuestInviteTimestamp,
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
    assert_eq!(
        FaultingRepository::new(inner, 1).schema_version(),
        Ok(blobyard_repository_sqlite::current_schema_version())
    );
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

#[test]
fn every_yard_session_operation_fails_at_its_repository_seam() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let inner: Arc<dyn Repository> = Arc::new(
        blobyard_repository_sqlite::SqliteRepository::open(
            &temporary.path().join("metadata.sqlite3"),
        )
        .expect("repository"),
    );
    let continuation = NewYardContinuation {
        id: "yardcont_fixture".to_owned(),
        continuation_hash: "a".repeat(64),
        code_hash: "b".repeat(64),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: 1,
        expires_at_ms: 2,
    };
    let session = NewYardSession {
        id: "yardsession_fixture".to_owned(),
        token_hash: "c".repeat(64),
        created_at_ms: 1,
        expires_at_ms: 2,
    };
    let audit = YardSessionAuditContext {
        id: "audit_fixture".to_owned(),
        request_id: "request_fixture".to_owned(),
    };
    let event = NewAuditEvent {
        id: "audit_revoke_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "user_fixture".to_owned(),
        action: "yard.session_revoked".to_owned(),
        request_id: "request_revoke_fixture".to_owned(),
        target_type: "yard_session".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    };
    assert_yard_session_operation_failures(&inner, &continuation, &session, &audit, &event);
}

fn assert_yard_session_operation_failures(
    inner: &Arc<dyn Repository>,
    continuation: &NewYardContinuation,
    session: &NewYardSession,
    audit: &YardSessionAuditContext,
    event: &NewAuditEvent,
) {
    let faulted = || FaultingRepository::new(Arc::clone(inner), 0);
    assert_eq!(
        faulted().evaluate_yard_admission("docs-fixture", "user_fixture", 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().issue_yard_exchange_code(continuation),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            session,
            audit,
            1,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().list_yard_sessions(&continuation.yard_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().revoke_yard_session(&continuation.yard_id, &session.id, 1, event,),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().revoke_yard_session_by_token(&session.token_hash, &continuation.host_label, 1,),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().purge_yard_session_history(1),
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

#[path = "repository_fault_tests/yard_identity_tests.rs"]
mod yard_identity_tests;
