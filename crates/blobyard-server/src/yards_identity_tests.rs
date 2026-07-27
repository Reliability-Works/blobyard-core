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

fn assignment(created_at_ms: u64, updated_at_ms: u64) -> YardManagementRoleAssignment {
    YardManagementRoleAssignment {
        yard_id: "yard_fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        role: YardManagementRole::Owner,
        created_at_ms,
        updated_at_ms,
    }
}

fn policy(approved_at_ms: u64) -> YardApplicationPolicyRecord {
    YardApplicationPolicyRecord {
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
        approved_at_ms,
        approved_by_principal: "operator_fixture".to_owned(),
    }
}

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
fn identity_presentations_map_policy_fields_and_roles() {
    let presented = api_policy(policy(1)).expect("policy");
    assert_eq!(presented.revision, 3);
    assert_eq!(presented.approved_by_principal_id, "operator_fixture");
    assert_eq!(
        role_json(&["editor".to_owned(), "viewer".to_owned()]),
        r#"["editor","viewer"]"#
    );
}

#[test]
fn assignment_presentations_reject_unrepresentable_timestamps() {
    assert_eq!(
        error_status(api_assignment(assignment(u64::MAX, u64::MAX))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(api_assignment(assignment(1, u64::MAX))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn policy_presentations_reject_unrepresentable_timestamps() {
    assert_eq!(
        error_status(api_policy(policy(u64::MAX))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
