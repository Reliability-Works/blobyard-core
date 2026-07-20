use super::success;
use crate::adapter::yard_validation;
use blobyard_contract::{AuditValue, NewAuditEvent, NewWebYard, NewYardDeploy, RepositoryError};
use blobyard_core::Slug;

fn yard() -> NewWebYard {
    NewWebYard {
        id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: success(Slug::new("docs")),
        host_label: "docs-123456789-fixture".to_owned(),
        created_at_ms: 10,
    }
}

pub(super) fn deploy() -> NewYardDeploy {
    NewYardDeploy {
        id: "yarddeploy_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        client_deploy_id: "clientdeploy0001".to_owned(),
        manifest_root: ".blobyard-yard/yard_fixture/clientdeploy0001/".to_owned(),
        deployment_host_label: "docs-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        created_at_ms: 11,
    }
}

fn event(yard: &NewWebYard) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_yard_created".to_owned(),
        workspace_id: yard.workspace_id.clone(),
        actor: "fixture".to_owned(),
        action: "yard.created".to_owned(),
        request_id: "request_yard_created".to_owned(),
        target_type: "web_yard".to_owned(),
        metadata: vec![("yardId".to_owned(), AuditValue::String(yard.id.clone()))],
        created_at_ms: yard.created_at_ms,
    }
}

fn assert_start_error(
    yard: &NewWebYard,
    deploy: &NewYardDeploy,
    event: &NewAuditEvent,
    expected: RepositoryError,
) {
    assert_eq!(yard_validation::start(yard, deploy, event), Err(expected));
}

#[test]
fn start_validation_binds_every_identity_and_derives_both_times() {
    let yard = yard();
    let deploy = deploy();
    assert_eq!(
        yard_validation::start(&yard, &deploy, &event(&yard)),
        Ok((10, 11))
    );
    let mut wrong_yard = deploy.clone();
    wrong_yard.yard_id = "yard_foreign".to_owned();
    let mut wrong_workspace = deploy.clone();
    wrong_workspace.workspace_id = "workspace_foreign".to_owned();
    let mut wrong_project = deploy;
    wrong_project.project_id = "project_foreign".to_owned();
    for invalid in [wrong_yard, wrong_workspace, wrong_project] {
        assert_start_error(
            &yard,
            &invalid,
            &event(&yard),
            RepositoryError::InvalidInput,
        );
    }
}

#[test]
fn start_validation_rejects_unsafe_public_and_idempotency_identifiers() {
    let yard = yard();
    for host in ["docs", "-docs-fixture", "docs-fixture-", "Docs-fixture"] {
        let mut invalid = yard.clone();
        invalid.host_label = host.to_owned();
        assert_start_error(
            &invalid,
            &deploy(),
            &event(&invalid),
            RepositoryError::InvalidInput,
        );
    }
    for client in ["short", "-clientdeploy0001", "client deploy 0001"] {
        let mut invalid = deploy();
        invalid.client_deploy_id = client.to_owned();
        assert_start_error(
            &yard,
            &invalid,
            &event(&yard),
            RepositoryError::InvalidInput,
        );
    }
    let mut invalid = deploy();
    invalid.deployment_host_label = "invalid".to_owned();
    assert_start_error(
        &yard,
        &invalid,
        &event(&yard),
        RepositoryError::InvalidInput,
    );
    let mut invalid = deploy();
    invalid.manifest_root = ".blobyard-yard/foreign/root/".to_owned();
    assert_start_error(
        &yard,
        &invalid,
        &event(&yard),
        RepositoryError::InvalidInput,
    );
}

#[test]
fn start_validation_rejects_mismatched_or_duplicate_audit_metadata() {
    let yard = yard();
    let deploy = deploy();
    let mut invalid = event(&yard);
    invalid.action = "yard.deleted".to_owned();
    assert_start_error(&yard, &deploy, &invalid, RepositoryError::InvalidInput);
    let mut duplicate = event(&yard);
    duplicate.metadata.push(duplicate.metadata[0].clone());
    assert_start_error(&yard, &deploy, &duplicate, RepositoryError::InvalidInput);
}

#[test]
fn start_validation_rejects_times_outside_sqlite_range() {
    let mut invalid_yard = yard();
    invalid_yard.created_at_ms = i64::MAX as u64 + 1;
    assert_start_error(
        &invalid_yard,
        &deploy(),
        &event(&invalid_yard),
        RepositoryError::InvalidInput,
    );
    let yard = yard();
    let mut invalid_deploy = deploy();
    invalid_deploy.created_at_ms = i64::MAX as u64 + 1;
    assert_start_error(
        &yard,
        &invalid_deploy,
        &event(&yard),
        RepositoryError::InvalidInput,
    );
}
