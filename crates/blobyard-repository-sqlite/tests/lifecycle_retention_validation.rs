#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Retention policy validation coverage.

/// Shared lifecycle repository fixtures.
#[path = "lifecycle_support/mod.rs"]
pub mod support;

use blobyard_contract::{LifecycleRepository, RepositoryError, RetentionPolicyRecord};
use support::{Fixture, deletion, event};

#[test]
fn retention_rejects_each_unsafe_policy_shape() {
    let fixture = Fixture::new();
    for policy in invalid_policies() {
        assert_eq!(
            fixture.repository.set_retention(
                &policy,
                &event(
                    "audit_invalid",
                    "retention.policy_set",
                    "request_policy",
                    10,
                ),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn retention_run_and_clear_inputs_fail_closed() {
    let fixture = Fixture::new();
    let policy = RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: None,
        branch_glob: None,
        created_at_ms: 10,
        updated_at_ms: 10,
    };
    fixture
        .repository
        .set_retention(
            &policy,
            &event("audit_policy", "retention.policy_set", "request_policy", 10),
        )
        .expect("retention policy");
    for (project, run, actor, request) in [
        ("", "run", "actor", "request"),
        ("project_fixture", "", "actor", "request"),
        ("project_fixture", "run", "", "request"),
        ("project_fixture", "run", "actor", ""),
    ] {
        assert_eq!(
            fixture
                .repository
                .begin_retention(project, run, actor, request, 11),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.begin_retention(
            "project_fixture",
            "run_overflow",
            "actor",
            "request",
            u64::MAX,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.fail_retention("", 12),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.retention_overview("missing_project"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.clear_retention(
            "project_fixture",
            12,
            &event("audit_wrong", "retention.policy_set", "request_clear", 12),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn retention_rolls_back_run_when_the_operation_identifier_conflicts() {
    let fixture = Fixture::new();
    fixture.store_complete("version_collision", "collision.txt", 1, 1, None);
    fixture
        .repository
        .begin_object_deletion(&deletion("retention_collision", "collision.txt", None))
        .expect("object deletion plan");
    let policy = RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: None,
        branch_glob: None,
        created_at_ms: 10,
        updated_at_ms: 10,
    };
    fixture
        .repository
        .set_retention(
            &policy,
            &event("audit_policy", "retention.policy_set", "request_policy", 10),
        )
        .expect("retention policy");
    assert_eq!(
        fixture.repository.begin_retention(
            "project_fixture",
            "retention_collision",
            "system:retention",
            "request_retention",
            11,
        ),
        Err(RepositoryError::Conflict)
    );
    assert!(
        fixture
            .repository
            .retention_overview("project_fixture")
            .expect("overview")
            .last_run
            .is_none()
    );
}

#[test]
fn retention_overview_maps_policy_storage_failure() {
    let fixture = Fixture::new();
    rusqlite::Connection::open(&fixture.path)
        .expect("fixture database")
        .execute("DROP TABLE retention_policies", [])
        .expect("drop policies table");
    assert_eq!(
        fixture.repository.retention_overview("project_fixture"),
        Err(RepositoryError::Unavailable)
    );
}

fn invalid_policies() -> Vec<RetentionPolicyRecord> {
    let valid = RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: None,
        branch_glob: None,
        created_at_ms: 10,
        updated_at_ms: 10,
    };
    vec![
        RetentionPolicyRecord {
            keep_latest: 0,
            ..valid.clone()
        },
        RetentionPolicyRecord {
            updated_at_ms: 9,
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some(String::new()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some(" padded".to_owned()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some("bad\\path".to_owned()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some("bad\npath".to_owned()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some("/absolute".to_owned()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            path_glob: Some("safe/../escape".to_owned()),
            ..valid.clone()
        },
        RetentionPolicyRecord {
            branch_glob: Some("x".repeat(257)),
            ..valid
        },
    ]
}
