use super::{FaultingRepository, Repository};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardIdentityRepository, YardManagementRole,
};
use blobyard_core::ApplicationPolicyGraph;
use std::{collections::BTreeMap, sync::Arc};

fn identity_repository_fixture(
    suffix: &str,
) -> (tempfile::TempDir, Arc<dyn Repository>, NewAuditEvent) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository = Arc::new(
        blobyard_repository_sqlite::SqliteRepository::open(
            &temporary.path().join("metadata.sqlite3"),
        )
        .expect("repository"),
    );
    let event = NewAuditEvent {
        id: format!("audit_identity_{suffix}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "user_fixture".to_owned(),
        action: "yard.management_role_set".to_owned(),
        request_id: format!("request_identity_{suffix}"),
        target_type: "yard_management_role".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    };
    (temporary, repository, event)
}

#[test]
fn every_yard_identity_operation_fails_at_its_repository_seam() {
    let (_temporary, inner, event) = identity_repository_fixture("fixture");
    let faulted = || FaultingRepository::new(Arc::clone(&inner), 0);
    assert_eq!(
        faulted().list_yard_management_roles("yard_fixture", None),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().set_yard_management_role(
            "yard_fixture",
            "user_fixture",
            YardManagementRole::Owner,
            1,
            &event,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().revoke_yard_management_role("yard_fixture", "user_fixture", 1, &event),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().get_yard_application_policy("yard_fixture"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().set_yard_application_policy(
            "yard_fixture",
            &"a".repeat(64),
            ApplicationPolicyGraph {
                default_role: None,
                roles: BTreeMap::new(),
            },
            "user_fixture",
            1,
            &event,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().set_yard_access_roles(
            "yard_fixture",
            "yardgrant_fixture",
            &["viewer".to_owned()],
            1,
            &event,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().resolve_yard_identity("docs-fixture", &"b".repeat(64), 1),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn every_yard_identity_operation_forwards_after_its_repository_seam() {
    let (_temporary, inner, event) = identity_repository_fixture("forward");
    let forwarding = || FaultingRepository::new(Arc::clone(&inner), 1);
    assert_eq!(
        forwarding().list_yard_management_roles("yard_fixture", None),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().set_yard_management_role(
            "yard_fixture",
            "user_fixture",
            YardManagementRole::Owner,
            1,
            &event,
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().revoke_yard_management_role("yard_fixture", "user_fixture", 1, &event),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().get_yard_application_policy("yard_fixture"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().set_yard_application_policy(
            "yard_fixture",
            &"a".repeat(64),
            ApplicationPolicyGraph {
                default_role: None,
                roles: BTreeMap::new(),
            },
            "user_fixture",
            1,
            &event,
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().set_yard_access_roles(
            "yard_fixture",
            "yardgrant_fixture",
            &["viewer".to_owned()],
            1,
            &event,
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        forwarding().resolve_yard_identity("docs-fixture", &"b".repeat(64), 1),
        Err(RepositoryError::NotFound)
    );
}
