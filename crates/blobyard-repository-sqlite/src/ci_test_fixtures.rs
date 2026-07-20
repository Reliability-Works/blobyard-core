#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::SqliteRepository;
use blobyard_contract::{
    CiAction, CiRepository, GithubOidcIdentity, LocalCiTrustRecord, MetadataRepository,
    NewAuditEvent, NewCiAuditEvent, NewMachineSession, ProjectRecord, WorkspaceRecord,
    ci_audit_event,
};
use blobyard_core::Slug;

pub(super) fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: Slug::new("fixture".to_owned()).expect("workspace slug"),
        })
        .expect("workspace");
    repository
        .create_project(&ProjectRecord {
            id: "project_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            name: "Project".to_owned(),
            slug: Slug::new("project".to_owned()).expect("project slug"),
        })
        .expect("project");
    repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace_foreign".to_owned(),
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign".to_owned()).expect("foreign slug"),
        })
        .expect("foreign workspace");
    repository
        .create_project(&ProjectRecord {
            id: "project_foreign".to_owned(),
            workspace_id: "workspace_foreign".to_owned(),
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign".to_owned()).expect("foreign project slug"),
        })
        .expect("foreign project");
    (temporary, repository)
}

pub(super) fn repository_with_trust() -> (tempfile::TempDir, SqliteRepository, LocalCiTrustRecord) {
    let (temporary, repository) = repository();
    let trust = trust("trust_fixture", None, 1);
    repository
        .create_ci_trust(&trust, &event("ci.trust_created", "ci_trust", &trust.id, 1))
        .expect("create trust");
    (temporary, repository, trust)
}

pub(super) fn repository_with_trust_and_session() -> (
    tempfile::TempDir,
    SqliteRepository,
    LocalCiTrustRecord,
    NewMachineSession,
) {
    let (temporary, repository, trust) = repository_with_trust();
    let session = session(1, 10);
    repository
        .mint_machine_session(
            &session,
            &event("ci.token_minted", "project", "project_fixture", 10),
        )
        .expect("mint machine session");
    (temporary, repository, trust, session)
}

pub(super) fn trust(id: &str, project_id: Option<&str>, created_at_ms: u64) -> LocalCiTrustRecord {
    blobyard_testkit::ci_trust(
        id,
        "workspace_fixture",
        project_id,
        "https://api.blobyard.local",
        created_at_ms,
    )
}

pub(super) fn session(index: u64, now_ms: u64) -> NewMachineSession {
    NewMachineSession {
        id: format!("machine_{index}"),
        token_prefix: format!("byd_ci_{index}"),
        secret_hash: checksum(index + 1_000),
        identity: GithubOidcIdentity {
            audience: "https://api.blobyard.local".to_owned(),
            repository: "reliability-works/blobyard-core".to_owned(),
            git_ref: "refs/heads/main".to_owned(),
            workflow_path: ".github/workflows/release.yml".to_owned(),
            workflow_ref: "refs/heads/main".to_owned(),
            environment: None,
            run_id: index.to_string(),
            run_attempt: Some("1".to_owned()),
            sha: Some(checksum(index)),
            expires_at_ms: now_ms + 600_000,
        },
        workspace: Some("fixture".to_owned()),
        project: "project".to_owned(),
        actions: vec![CiAction::Upload],
        oidc_token_hash: checksum(index + 2_000),
        now_ms,
    }
}

pub(super) fn event(
    action: &str,
    target_type: &str,
    target_id: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    ci_audit_event(NewCiAuditEvent {
        id: format!("event_{action}_{target_id}_{created_at_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "github:reliability-works/blobyard-core".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{target_id}_{created_at_ms}"),
        target_type: target_type.to_owned(),
        target_id: target_id.to_owned(),
        repository: blobyard_testkit::CI_REPOSITORY.to_owned(),
        created_at_ms,
    })
}

fn checksum(value: u64) -> String {
    format!("{value:064x}")
}
