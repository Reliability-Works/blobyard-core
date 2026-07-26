#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{decode_policy, parse_role, policy_line, read_policy, role_label, role_lines};
use crate::yard_commands::ApplicationPolicySetArgs;
use blobyard_api_client::{
    ListYardManagementRolesResponse, YardApplicationPolicy, YardApplicationPolicyResponse,
    YardManagementRole, YardManagementRoleAssignment,
};
use blobyard_core::{ApplicationPolicyGraph, ApplicationRoleDefinition, ErrorCode};
use std::{collections::BTreeMap, path::PathBuf};

fn graph() -> ApplicationPolicyGraph {
    ApplicationPolicyGraph {
        default_role: Some("viewer".to_owned()),
        roles: BTreeMap::from([(
            "viewer".to_owned(),
            ApplicationRoleDefinition {
                inherits: Vec::new(),
                permissions: vec!["content.read".to_owned()],
            },
        )]),
    }
}

fn arguments(policy: Option<PathBuf>, policy_json: Option<String>) -> ApplicationPolicySetArgs {
    ApplicationPolicySetArgs {
        name: "docs".to_owned(),
        policy,
        policy_json,
        source_manifest_digest: "a".repeat(64),
    }
}

#[test]
fn management_roles_round_trip_labels_and_render_pages() {
    for role in [
        YardManagementRole::Owner,
        YardManagementRole::Admin,
        YardManagementRole::Developer,
        YardManagementRole::Auditor,
    ] {
        assert_eq!(parse_role(role_label(role)).expect("role"), role);
    }
    assert_eq!(
        parse_role("reader").expect_err("unknown role").code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        role_lines(&ListYardManagementRolesResponse {
            items: Vec::new(),
            next_cursor: None,
        }),
        "No Yard management roles."
    );
    assert_eq!(
        role_lines(&ListYardManagementRolesResponse {
            items: vec![
                YardManagementRoleAssignment {
                    user_id: "user_owner".to_owned(),
                    role: YardManagementRole::Owner,
                    created_at: "1970-01-01T00:00:00.001Z".to_owned(),
                    updated_at: "1970-01-01T00:00:00.001Z".to_owned(),
                },
                YardManagementRoleAssignment {
                    user_id: "user_auditor".to_owned(),
                    role: YardManagementRole::Auditor,
                    created_at: "1970-01-01T00:00:00.001Z".to_owned(),
                    updated_at: "1970-01-01T00:00:00.001Z".to_owned(),
                },
            ],
            next_cursor: None,
        }),
        "owner\tuser_owner\nauditor\tuser_auditor"
    );
}

#[test]
fn policy_output_and_inputs_cover_empty_inline_and_file_paths() {
    assert_eq!(
        policy_line(&YardApplicationPolicyResponse { policy: None }),
        "No approved application policy."
    );
    assert_eq!(
        policy_line(&YardApplicationPolicyResponse {
            policy: Some(YardApplicationPolicy {
                revision: 2,
                source_manifest_digest: "a".repeat(64),
                graph: graph(),
                approved_at: "1970-01-01T00:00:00.001Z".to_owned(),
                approved_by_principal_id: "operator".to_owned(),
            }),
        }),
        "Application policy revision 2 with 1 roles."
    );

    let encoded = serde_json::to_string(&graph()).expect("policy json");
    assert_eq!(
        read_policy(&arguments(None, Some(encoded.clone()))).expect("inline"),
        graph()
    );
    let temporary = tempfile::tempdir().expect("temporary");
    let path = temporary.path().join("policy.json");
    std::fs::write(&path, &encoded).expect("policy file");
    assert_eq!(
        read_policy(&arguments(Some(path), None)).expect("file"),
        graph()
    );
}

#[test]
fn policy_input_failures_have_stable_error_classes() {
    assert_eq!(
        read_policy(&arguments(None, None))
            .expect_err("missing input")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        decode_policy(b"{").expect_err("malformed policy").code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        read_policy(&arguments(Some(PathBuf::from("missing-policy.json")), None,))
            .expect_err("missing file")
            .code(),
        ErrorCode::NotFound
    );
    let temporary = tempfile::tempdir().expect("temporary");
    assert_eq!(
        read_policy(&arguments(Some(temporary.path().to_path_buf()), None,))
            .expect_err("directory is not a policy file")
            .code(),
        ErrorCode::InternalError
    );
}
