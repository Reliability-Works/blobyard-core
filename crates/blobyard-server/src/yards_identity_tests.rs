#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::identity_presentation::{api_assignment, api_policy, api_role, domain_role, role_json};
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_api_client::YardManagementRole as ApiRole;
use blobyard_contract::{
    YardApplicationPolicyRecord, YardManagementRole, YardManagementRoleAssignment,
};
use blobyard_core::{ApplicationPolicyGraph, EffectiveApplicationPolicy};
use std::collections::BTreeMap;

#[test]
fn management_role_presentations_cover_every_variant() {
    for (domain, api) in [
        (YardManagementRole::Owner, ApiRole::Owner),
        (YardManagementRole::Admin, ApiRole::Admin),
        (YardManagementRole::Developer, ApiRole::Developer),
        (YardManagementRole::Auditor, ApiRole::Auditor),
    ] {
        assert_eq!(domain_role(api), domain);
        assert_eq!(api_role(domain), api);
        let presented = api_assignment(YardManagementRoleAssignment {
            yard_id: "yard_fixture".to_owned(),
            user_id: "user_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            role: domain,
            created_at_ms: 1,
            updated_at_ms: 2,
        })
        .expect("assignment");
        assert_eq!(presented.role, api);
        assert_eq!(presented.user_id, "user_fixture");
    }
}

#[test]
fn identity_presentations_map_timestamps_and_policy_fields() {
    let policy = api_policy(YardApplicationPolicyRecord {
        yard_id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        revision: 3,
        source_manifest_digest: "a".repeat(64),
        policy: ApplicationPolicyGraph {
            default_role: None,
            roles: BTreeMap::new(),
        },
        effective: EffectiveApplicationPolicy {
            effective_roles: BTreeMap::new(),
            effective_permissions: BTreeMap::new(),
        },
        approved_at_ms: 1,
        approved_by_principal: "operator_fixture".to_owned(),
    })
    .expect("policy");
    assert_eq!(policy.revision, 3);
    assert_eq!(policy.approved_by_principal_id, "operator_fixture");
    assert_eq!(
        role_json(&["editor".to_owned(), "viewer".to_owned()]).expect("roles"),
        r#"["editor","viewer"]"#
    );
    assert_eq!(
        error_status(api_assignment(YardManagementRoleAssignment {
            yard_id: "yard_fixture".to_owned(),
            user_id: "user_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            role: YardManagementRole::Owner,
            created_at_ms: u64::MAX,
            updated_at_ms: u64::MAX,
        })),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
