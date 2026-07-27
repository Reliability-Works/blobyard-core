#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::SqliteRepository;
use blobyard_contract::NewAuditEvent;
use blobyard_core::{
    ApplicationPolicyGraph, ApplicationRoleDefinition, canonicalize_application_policy,
};
use rusqlite::{Connection, params};
use std::collections::BTreeMap;

pub(super) const YARD_ID: &str = "yard_identity_fixture";
pub(super) const ENVIRONMENT_ID: &str = "yardenv_identity_fixture";
pub(super) const USER_ID: &str = "user_identity_fixture";
pub(super) const HOST: &str = "identity-123456789-fixture";
pub(super) const TOKEN_HASH: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let (temporary, repository) = super::stable_behavior::repository();
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "INSERT INTO local_users
               (id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms)
             VALUES
               ('user_identity_fixture', 'workspace_fixture', 'Identity Fixture',
                'identity@example.test', 'active', 1, NULL),
               ('user_backup_owner', 'workspace_fixture', 'Backup Owner',
                'backup@example.test', 'active', 1, NULL);
             INSERT INTO web_yards
               (id, workspace_id, project_id, name, host_label, current_deploy_id,
                status, created_at_ms, updated_at_ms, deleted_at_ms)
             VALUES
               ('yard_identity_fixture', 'workspace_fixture', 'project_fixture', 'identity',
                'identity-123456789-fixture', NULL, 'active', 1, 1, NULL),
               ('yard_identity_inactive', 'workspace_fixture', 'project_fixture', 'inactive-id',
                'inactive-123456789-fixture', NULL, 'suspended', 1, 1, NULL);
             INSERT INTO yard_environments
               (id, yard_id, name, kind, status, created_at_ms, updated_at_ms, deleted_at_ms)
             VALUES
               ('yardenv_identity_fixture', 'yard_identity_fixture', 'production',
                'production', 'active', 1, 1, NULL);
             INSERT INTO yard_access_policies
               (yard_id, visibility, updated_at_ms, updated_by_principal)
             VALUES ('yard_identity_fixture', 'selected', 1, 'fixture');
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             VALUES
               ('yardgrant_identity', 'yard_identity_fixture', NULL, 'user',
                'user_identity_fixture', '[]', 'active', 1, 'fixture', NULL, NULL);
             INSERT INTO yard_sessions
               (id, token_hash, yard_id, environment_id, host_label, user_id,
                created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms)
             VALUES
               ('yardsession_identity',
                'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                'yard_identity_fixture', 'yardenv_identity_fixture',
                'identity-123456789-fixture', 'user_identity_fixture',
                1, 1000, NULL, NULL);",
        )
        .expect("identity fixture");
    (temporary, repository)
}

pub(super) fn graph(default_role: Option<&str>) -> ApplicationPolicyGraph {
    ApplicationPolicyGraph {
        default_role: default_role.map(str::to_owned),
        roles: BTreeMap::from([(
            "viewer".to_owned(),
            ApplicationRoleDefinition {
                inherits: Vec::new(),
                permissions: vec!["content.read".to_owned()],
            },
        )]),
    }
}

pub(super) fn install_policy(connection: &Connection, default_role: Option<&str>) {
    let canonical = canonicalize_application_policy(graph(default_role)).expect("canonical policy");
    connection
        .execute(
            "INSERT OR REPLACE INTO yard_application_policies
               (yard_id, workspace_id, revision, source_manifest_digest, policy_json,
                effective_json, approved_at_ms, approved_by_principal)
             VALUES (?1, 'workspace_fixture', 1, ?2, ?3, ?4, 2, 'fixture')",
            params![
                YARD_ID,
                "b".repeat(64),
                serde_json::to_string(&canonical.graph).expect("graph"),
                serde_json::to_string(&canonical.effective).expect("effective"),
            ],
        )
        .expect("policy fixture");
}

pub(super) fn bad_event(action: &str, target_type: &str) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_bad_{action}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_bad_{action}"),
        target_type: target_type.to_owned(),
        metadata: Vec::new(),
        created_at_ms: 10,
    }
}

pub(super) fn access_roles_event(from: &[&str], to: &[&str], at: u64) -> NewAuditEvent {
    let encode = |roles: &[&str]| {
        serde_json::Value::Array(
            roles
                .iter()
                .map(|role| serde_json::Value::String((*role).to_owned()))
                .collect(),
        )
        .to_string()
    };
    let mut metadata = vec![
        (
            "from".to_owned(),
            blobyard_contract::AuditValue::String(encode(from)),
        ),
        (
            "grantId".to_owned(),
            blobyard_contract::AuditValue::String("yardgrant_identity".to_owned()),
        ),
        (
            "to".to_owned(),
            blobyard_contract::AuditValue::String(encode(to)),
        ),
        (
            "yardId".to_owned(),
            blobyard_contract::AuditValue::String(YARD_ID.to_owned()),
        ),
    ];
    metadata.sort_by(|left, right| left.0.cmp(&right.0));
    NewAuditEvent {
        id: format!("audit_access_roles_{at}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "yard.access_roles_set".to_owned(),
        request_id: format!("request_access_roles_{at}"),
        target_type: "yard_access_grant".to_owned(),
        metadata,
        created_at_ms: at,
    }
}

pub(super) fn insert_owner(connection: &Connection, user_id: &str) {
    connection
        .execute(
            "INSERT INTO yard_management_role_assignments
               (yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 'workspace_fixture', 'owner', 1, 1)",
            params![YARD_ID, user_id],
        )
        .expect("owner fixture");
}
