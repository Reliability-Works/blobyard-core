#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Retention planning, failure, resume, and completion coverage.

/// Shared lifecycle repository fixtures.
#[path = "lifecycle_support/mod.rs"]
pub mod support;

use blobyard_contract::{
    DeletionPlan, LifecycleRepository, RepositoryError, RetentionPolicyRecord,
};
use support::{Fixture, event};

#[test]
fn retention_validates_filters_resumes_and_completes() {
    let fixture = populated_fixture();
    assert!(
        fixture
            .repository
            .retention_overview("project_fixture")
            .expect("empty overview")
            .policy
            .is_none()
    );
    let policy = policy();
    install_policy(&fixture, &policy);
    let plan = begin_and_fail_retention(&fixture, &policy);
    resume_and_finish_retention(&fixture, &plan);
    clear_policy(&fixture);
}

fn install_policy(fixture: &Fixture, policy: &RetentionPolicyRecord) {
    assert_eq!(
        fixture.repository.set_retention(
            policy,
            &event(
                "audit_bad",
                "retention.policy_cleared",
                "request_policy",
                10
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let mut wrong_workspace = event(
        "audit_bad_workspace",
        "retention.policy_set",
        "request_policy",
        10,
    );
    "workspace_other".clone_into(&mut wrong_workspace.workspace_id);
    assert_eq!(
        fixture.repository.set_retention(policy, &wrong_workspace),
        Err(RepositoryError::InvalidInput)
    );
    fixture
        .repository
        .set_retention(
            policy,
            &event("audit_policy", "retention.policy_set", "request_policy", 10),
        )
        .expect("retention policy");
    assert_eq!(
        fixture.repository.retained_projects().expect("projects"),
        vec!["project_fixture"]
    );
}

fn begin_and_fail_retention(fixture: &Fixture, policy: &RetentionPolicyRecord) -> DeletionPlan {
    let plan = fixture
        .repository
        .begin_retention(
            "project_fixture",
            "retention_one",
            "system:retention",
            "request_retention",
            11,
        )
        .expect("retention plan");
    assert_eq!(plan.items.len(), 2);
    assert!(
        plan.items
            .iter()
            .all(|item| item.version_id.starts_with("release_"))
    );
    assert_pending_conflicts(fixture, policy);
    fixture
        .repository
        .fail_retention(&plan.id, 12)
        .expect("failed run");
    let failed = last_run(fixture);
    assert_eq!(failed.status, "failed");
    assert!(failed.error_summary.is_some());
    plan
}

fn resume_and_finish_retention(fixture: &Fixture, plan: &DeletionPlan) {
    let resumed = fixture
        .repository
        .begin_retention(
            "project_fixture",
            "retention_ignored",
            "system:retention",
            "request_ignored",
            13,
        )
        .expect("resumed plan");
    assert_eq!(resumed.id, plan.id);
    assert_eq!(resumed.items, plan.items);
    assert_eq!(last_run(fixture).status, "running");
    let wrong = event(
        "audit_wrong_finish",
        "object.deleted",
        "request_retention",
        14,
    );
    assert_eq!(
        fixture.repository.finish_deletion(&plan.id, 14, &wrong),
        Err(RepositoryError::InvalidInput)
    );
    fixture
        .repository
        .finish_deletion(
            &plan.id,
            14,
            &event(
                "audit_retention",
                "retention.enforced",
                "request_retention",
                14,
            ),
        )
        .expect("completed retention");
    let complete = last_run(fixture);
    assert_eq!(
        (
            complete.status.as_str(),
            complete.candidate_count,
            complete.deleted_count
        ),
        ("complete", 3, 2)
    );
    assert_eq!(
        fixture.repository.fail_retention(&plan.id, 15),
        Err(RepositoryError::NotFound)
    );
}

fn populated_fixture() -> Fixture {
    let fixture = Fixture::new();
    for (id, path, version, time, branch) in [
        ("release_old", "artifacts/app.zip", 1, 1, Some("release-a")),
        ("release_new", "artifacts/app.zip", 2, 2, Some("release-b")),
        (
            "release_other",
            "artifacts/other.zip",
            1,
            3,
            Some("release-c"),
        ),
        ("no_branch", "artifacts/no-branch.zip", 1, 4, None),
        ("wrong_branch", "artifacts/dev.zip", 1, 5, Some("main")),
        (
            "protected_preview",
            ".blobyard-preview/index.html",
            1,
            6,
            Some("release-d"),
        ),
        (
            "protected_yard",
            ".blobyard-yard/index.html",
            1,
            7,
            Some("release-e"),
        ),
    ] {
        fixture.store_complete(id, path, version, time, branch);
    }
    fixture
}

fn assert_pending_conflicts(fixture: &Fixture, policy: &RetentionPolicyRecord) {
    assert_eq!(
        fixture.repository.set_retention(
            policy,
            &event(
                "audit_pending",
                "retention.policy_set",
                "request_pending",
                11
            ),
        ),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.clear_retention(
            "project_fixture",
            11,
            &event(
                "audit_clear_pending",
                "retention.policy_cleared",
                "request_pending",
                11
            ),
        ),
        Err(RepositoryError::Conflict)
    );
}

fn clear_policy(fixture: &Fixture) {
    assert!(
        fixture
            .repository
            .clear_retention(
                "project_fixture",
                15,
                &event(
                    "audit_clear",
                    "retention.policy_cleared",
                    "request_clear",
                    15
                ),
            )
            .expect("cleared policy")
    );
    assert!(
        !fixture
            .repository
            .clear_retention(
                "project_fixture",
                16,
                &event(
                    "audit_clear_again",
                    "retention.policy_cleared",
                    "request_clear_again",
                    16
                ),
            )
            .expect("already clear")
    );
    assert_eq!(
        fixture.repository.retention_policy("project_fixture"),
        Err(RepositoryError::NotFound)
    );
    assert!(
        fixture
            .repository
            .retained_projects()
            .expect("no projects")
            .is_empty()
    );
    assert_eq!(
        fixture.repository.begin_retention(
            "project_fixture",
            "retention_without_policy",
            "system:retention",
            "request_without_policy",
            17,
        ),
        Err(RepositoryError::NotFound)
    );
}

fn last_run(fixture: &Fixture) -> blobyard_contract::RetentionRunRecord {
    fixture
        .repository
        .retention_overview("project_fixture")
        .expect("overview")
        .last_run
        .expect("run")
}

fn policy() -> RetentionPolicyRecord {
    RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: Some("**/**".to_owned()),
        branch_glob: Some("release-?".to_owned()),
        created_at_ms: 10,
        updated_at_ms: 10,
    }
}
