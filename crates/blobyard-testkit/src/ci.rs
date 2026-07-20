use blobyard_contract::{CiAction, GithubOidcIdentity, LocalCiTrustRecord};

/// Canonical non-secret repository identity used by Core CI fixtures.
pub const CI_REPOSITORY: &str = "reliability-works/blobyard-core";

/// Builds one canonical non-secret CI trust fixture.
#[must_use]
pub fn ci_trust(
    id: &str,
    workspace_id: &str,
    project_id: Option<&str>,
    audience: &str,
    created_at_ms: u64,
) -> LocalCiTrustRecord {
    LocalCiTrustRecord {
        id: id.to_owned(),
        workspace_id: workspace_id.to_owned(),
        project_id: project_id.map(str::to_owned),
        repository: CI_REPOSITORY.to_owned(),
        workflow_path: ".github/workflows/release.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        allowed_ref_glob: "refs/heads/main".to_owned(),
        environment: None,
        allowed_actions: vec![CiAction::Upload],
        audience: audience.to_owned(),
        created_at_ms,
        revoked_at_ms: None,
    }
}

/// Builds one canonical verified GitHub OIDC identity fixture.
#[must_use]
pub fn github_oidc_identity(
    audience: &str,
    run_id: &str,
    expires_at_ms: u64,
) -> GithubOidcIdentity {
    GithubOidcIdentity {
        audience: audience.to_owned(),
        repository: CI_REPOSITORY.to_owned(),
        git_ref: "refs/heads/main".to_owned(),
        workflow_path: ".github/workflows/release.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        environment: None,
        run_id: run_id.to_owned(),
        run_attempt: Some("1".to_owned()),
        sha: Some("a".repeat(40)),
        expires_at_ms,
    }
}
