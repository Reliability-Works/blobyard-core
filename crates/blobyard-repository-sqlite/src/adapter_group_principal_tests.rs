#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{NewYardAccessGrant, YardAccessPrincipalKind};

#[test]
fn link_principals_remain_inert_during_grant_validation() {
    let mut connection = rusqlite::Connection::open_in_memory().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    let yard = blobyard_contract::WebYardRecord {
        id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: blobyard_core::Slug::new("fixture".to_owned()).expect("slug"),
        host_label: "fixture".to_owned(),
        current_deploy_id: None,
        status: blobyard_contract::WebYardStatus::Active,
        created_at_ms: 1,
        updated_at_ms: 1,
        deleted_at_ms: None,
    };
    let grant = NewYardAccessGrant {
        id: "grant_link_fixture".to_owned(),
        yard_id: yard.id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Link,
        principal_id: "link_fixture".to_owned(),
        app_roles: Vec::new(),
        created_at_ms: 1,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    assert_eq!(
        super::super::yard_access_principals::validate(&transaction, &yard, &grant),
        Ok(())
    );
}
