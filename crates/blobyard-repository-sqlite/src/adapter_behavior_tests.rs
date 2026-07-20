#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::SqliteRepository;
use blobyard_contract::{
    LifecycleRepository, MetadataRepository, NewAuditEvent, NewObjectDeletion, NewObjectVersion,
    ObjectDeletionTarget, ProjectRecord, RepositoryError, RetentionPolicyRecord, WorkspaceRecord,
};
use blobyard_core::Slug;

#[test]
fn deletion_failures_and_replays_keep_their_exact_contract() {
    let (_temporary, repository) = repository();
    assert_eq!(
        repository.begin_object_deletion(&deletion("delete_missing", "missing.bin")),
        Err(RepositoryError::NotFound)
    );
    repository
        .reserve_object_version(&NewObjectVersion {
            id: "pending_version".to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "pending.bin".to_owned(),
            version: 1,
            storage_key: "objects/pending_version".to_owned(),
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        })
        .expect("pending object version");
    assert_eq!(
        repository.begin_object_deletion(&deletion("delete_pending", "pending.bin")),
        Err(RepositoryError::Conflict)
    );
    store(&repository, "version_one", 1);
    store(&repository, "version_two", 2);
    let plan = repository
        .begin_object_deletion(&deletion("delete_one", "object.bin"))
        .expect("deletion plan");
    assert_eq!(
        repository.finish_deletion(
            &plan.id,
            3,
            &event(
                "wrong_delete",
                "retention.enforced",
                "fixture",
                "request_delete"
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let delete_event = event(
        "delete_event",
        "object.deleted",
        "fixture",
        "request_delete",
    );
    repository
        .finish_deletion(&plan.id, 3, &delete_event)
        .expect("finish deletion");
    repository
        .finish_deletion(&plan.id, 4, &delete_event)
        .expect("replay deletion");
}

#[test]
fn retention_failures_and_replays_keep_their_exact_contract() {
    let (_temporary, repository) = repository();
    store(&repository, "retained_one", 1);
    store(&repository, "retained_two", 2);
    let policy = RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: Some("**/**".to_owned()),
        branch_glob: None,
        created_at_ms: 5,
        updated_at_ms: 5,
    };
    repository
        .set_retention(
            &policy,
            &event(
                "set_event",
                "retention.policy_set",
                "fixture",
                "request_set",
            ),
        )
        .expect("retention policy");
    assert_eq!(
        repository.retained_projects().expect("retained projects"),
        vec!["project_fixture"]
    );
    assert_eq!(
        repository.fail_retention("missing", 6),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.begin_retention("", "run", "actor", "request", 6),
        Err(RepositoryError::InvalidInput)
    );
    let retention = repository
        .begin_retention(
            "project_fixture",
            "run_one",
            "system:retention",
            "request_retention",
            6,
        )
        .expect("retention plan");
    assert_eq!(
        repository
            .begin_retention(
                "project_fixture",
                "run_two",
                "system:retention",
                "request_retention",
                7,
            )
            .expect("resumed plan")
            .id,
        retention.id
    );
    assert_eq!(
        repository.set_retention(
            &policy,
            &event("wrong_event", "wrong", "fixture", "request_wrong"),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.clear_retention(
            "project_fixture",
            7,
            &event(
                "clear_pending",
                "retention.policy_cleared",
                "fixture",
                "request_clear",
            ),
        ),
        Err(RepositoryError::Conflict)
    );
    repository
        .finish_deletion(
            &retention.id,
            7,
            &event(
                "retention_event",
                "retention.enforced",
                "system:retention",
                "request_retention",
            ),
        )
        .expect("finish retention");
    let clear_event = event(
        "clear_event",
        "retention.policy_cleared",
        "fixture",
        "request_clear",
    );
    assert!(
        repository
            .clear_retention("project_fixture", 8, &clear_event)
            .expect("clear policy")
    );
    assert!(
        !repository
            .clear_retention("project_fixture", 9, &clear_event)
            .expect("replay clear")
    );
}

pub(super) fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    seed_namespaces(&repository);
    (temporary, repository)
}

fn seed_namespaces(repository: &SqliteRepository) {
    repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: Slug::new("fixture").expect("workspace slug"),
        })
        .expect("workspace");
    repository
        .create_project(&ProjectRecord {
            id: "project_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: Slug::new("project").expect("project slug"),
        })
        .expect("project");
}

fn store(repository: &SqliteRepository, id: &str, version: u64) {
    repository
        .reserve_object_version(&NewObjectVersion {
            id: id.to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "object.bin".to_owned(),
            version,
            storage_key: format!("objects/{id}"),
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        })
        .expect("object version");
    repository
        .complete_object_version(id, 1, &checksum('a'))
        .expect("complete object");
}

fn deletion(id: &str, path: &str) -> NewObjectDeletion {
    NewObjectDeletion {
        id: id.to_owned(),
        target: ObjectDeletionTarget {
            project_id: "project_fixture".to_owned(),
            object_path: path.to_owned(),
            version: None,
        },
        actor: "fixture".to_owned(),
        request_id: "request_delete".to_owned(),
        created_at_ms: 2,
    }
}

fn event(id: &str, action: &str, actor: &str, request_id: &str) -> NewAuditEvent {
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: actor.to_owned(),
        action: action.to_owned(),
        request_id: request_id.to_owned(),
        target_type: "fixture".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 3,
    }
}

pub(super) fn checksum(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}
