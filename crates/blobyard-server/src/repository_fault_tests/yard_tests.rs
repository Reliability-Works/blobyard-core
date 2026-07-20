#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FaultingRepository, Repository};
use crate::transfers::test_seams;
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError, WebYardRepository,
};
use blobyard_core::Slug;
use std::sync::Arc;

fn yard() -> NewWebYard {
    NewWebYard {
        id: "yard_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("fault-site").expect("yard name"),
        host_label: "fault-site-123456789-fixture".to_owned(),
        created_at_ms: 1,
    }
}

fn yard_fixture() -> blobyard_testkit::YardConformanceFixture {
    blobyard_testkit::YardConformanceFixture::new("docs", "inactive", "history")
        .expect("Yard conformance fixture")
}

fn deploy() -> NewYardDeploy {
    NewYardDeploy {
        id: "yarddeploy_fault".to_owned(),
        yard_id: "yard_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        client_deploy_id: "client-deploy-fault".to_owned(),
        manifest_root: ".blobyard-yard/yard_fault/client-deploy-fault/".to_owned(),
        deployment_host_label: "fault-site-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        created_at_ms: 1,
    }
}

fn event(action: &str, target_type: &str, field: &str, value: &str) -> NewAuditEvent {
    blobyard_testkit::yard_event(action, target_type, field, value, 1)
}

#[test]
fn yard_fault_wrapper_forwards_every_operation() {
    let (_temporary, inner) = super::conforming_repository();
    let repository = FaultingRepository::new(inner, usize::MAX);
    blobyard_testkit::yard_conformance(&repository, &yard_fixture()).expect("yard conformance");
    assert!(
        !repository
            .list_yard_deploys("yard_docs_1")
            .expect("deploy list")
            .is_empty()
    );
    assert!(
        repository
            .pending_yard_cleanups(None)
            .expect("pending cleanup list")
            .is_empty()
    );
}

#[test]
fn yard_fault_wrapper_fails_every_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let fail = || FaultingRepository::new(Arc::clone(&inner), 0);
    let yard = yard();
    let deploy = deploy();
    let created = event("yard.created", "web_yard", "yardId", &yard.id);
    let deployed = event("yard.deployed", "yard_deploy", "deployId", &deploy.id);
    let rolled_back = event("yard.rolled_back", "yard_deploy", "yardId", &yard.id);
    let deleted = event("yard.deleted", "web_yard", "yardId", &yard.id);
    let files = [NewYardFile {
        normalized_path: "index.html".to_owned(),
        version_id: "version_fault".to_owned(),
        byte_size: 1,
    }];
    assert_eq!(
        fail().start_yard_deploy(&yard, &deploy, &created),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().list_web_yards(&yard.project_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().web_yard_by_id(&yard.id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().list_yard_deploys(&yard.id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().yard_deploy_by_id(&deploy.id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().finalise_yard_deploy(&deploy.id, &files, 1, &deployed),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().fail_yard_deploy(&deploy.id, "UPLOAD_FAILED", "failed", 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().rollback_web_yard(&yard.id, Some(&deploy.id), 1, &rolled_back),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().delete_web_yard(&yard.id, 1, &deleted),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().pending_yard_cleanups(Some(&yard.id)),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().yard_file_by_host(&yard.host_label, ""),
        Err(RepositoryError::Unavailable)
    );
}
