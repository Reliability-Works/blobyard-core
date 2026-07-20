#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Durable object deletion validation and replay coverage.

/// Shared lifecycle repository fixtures.
#[path = "lifecycle_support/mod.rs"]
pub mod support;

use blobyard_contract::{DeletionPlan, LifecycleRepository, MetadataRepository, RepositoryError};
use support::{Fixture, deletion, event};

#[test]
fn object_deletion_is_validated_retryable_and_idempotent() {
    let fixture = Fixture::new();
    fixture.store_complete("version_one", "delete/me.txt", 1, 1, None);
    fixture.store_aborted("version_two", "delete/me.txt", 2);
    fixture.store_pending("version_pending", "pending.txt", 1);
    assert_deletion_input_validation(&fixture);
    let plan = begin_and_replay_deletion(&fixture);
    finish_and_replay_deletion(&fixture, &plan);
}

fn assert_deletion_input_validation(fixture: &Fixture) {
    for version in [Some(0), Some(u64::MAX)] {
        assert_eq!(
            fixture.repository.begin_object_deletion(&deletion(
                "delete_invalid",
                "delete/me.txt",
                version,
            )),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture
            .repository
            .begin_object_deletion(&deletion("delete_missing", "missing.txt", None,)),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .repository
            .begin_object_deletion(&deletion("delete_pending", "pending.txt", None,)),
        Err(RepositoryError::Conflict)
    );
}

fn begin_and_replay_deletion(fixture: &Fixture) -> DeletionPlan {
    let plan = fixture
        .repository
        .begin_object_deletion(&deletion("delete_one", "delete/me.txt", None))
        .expect("deletion plan");
    assert_eq!(plan.items.len(), 1);
    assert!(!plan.complete);
    let replay = fixture
        .repository
        .begin_object_deletion(&deletion("delete_retry", "delete/me.txt", None))
        .expect("replayed plan");
    assert_eq!(replay.id, plan.id);
    plan
}

fn finish_and_replay_deletion(fixture: &Fixture, plan: &DeletionPlan) {
    let wrong = event("audit_wrong", "retention.enforced", "request_delete", 2);
    assert_eq!(
        fixture.repository.finish_deletion(&plan.id, 2, &wrong),
        Err(RepositoryError::InvalidInput)
    );
    assert!(fixture.repository.object_version("version_one").is_ok());
    let audit = event("audit_delete", "object.deleted", "request_delete", 2);
    assert_eq!(
        fixture
            .repository
            .finish_deletion(&plan.id, u64::MAX, &audit),
        Err(RepositoryError::InvalidInput)
    );
    assert!(fixture.repository.object_version("version_one").is_ok());
    fixture
        .repository
        .finish_deletion(&plan.id, 2, &audit)
        .expect("finished deletion");
    fixture
        .repository
        .finish_deletion(&plan.id, 3, &audit)
        .expect("idempotent finish");
    for id in ["version_one", "version_two"] {
        assert_eq!(
            fixture.repository.object_version(id),
            Err(RepositoryError::NotFound)
        );
    }
    assert!(
        fixture
            .repository
            .begin_object_deletion(&deletion("delete_after", "delete/me.txt", None))
            .expect("completed replay")
            .complete
    );
    assert_eq!(
        fixture.repository.finish_deletion("", 3, &audit),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .finish_deletion("delete_unknown", 3, &audit),
        Err(RepositoryError::NotFound)
    );
}
