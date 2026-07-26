use blobyard_contract::{
    AuditValue, NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardDeploy,
    YardAccessPrincipalKind,
};
use blobyard_core::{ApplicationPolicyGraph, ApplicationRoleDefinition, Slug};
use std::collections::BTreeMap;

/// Builds the deterministic approved policy used by Yard repository conformance.
#[must_use]
pub fn yard_application_policy() -> ApplicationPolicyGraph {
    ApplicationPolicyGraph {
        default_role: None,
        roles: BTreeMap::from([
            (
                "editor".to_owned(),
                ApplicationRoleDefinition {
                    inherits: vec!["viewer".to_owned()],
                    permissions: vec!["content.write".to_owned()],
                },
            ),
            (
                "viewer".to_owned(),
                ApplicationRoleDefinition {
                    inherits: Vec::new(),
                    permissions: vec!["content.read".to_owned()],
                },
            ),
        ]),
    }
}

/// Builds the deterministic owner-assignment audit used by Yard conformance.
#[must_use]
pub fn yard_owner_event(yard_id: &str, user_id: &str, at: u64) -> NewAuditEvent {
    let mut event = event(
        "yard.management_role_set",
        "yard_management_role",
        "yardId",
        yard_id,
        at,
    );
    event.metadata.extend([
        ("from".to_owned(), AuditValue::Null),
        ("to".to_owned(), AuditValue::String("owner".to_owned())),
        ("userId".to_owned(), AuditValue::String(user_id.to_owned())),
    ]);
    event
}

/// Builds the deterministic policy-approval audit used by Yard conformance.
#[must_use]
pub fn yard_policy_event(yard_id: &str, digest: &str, at: u64) -> NewAuditEvent {
    let mut event = event(
        "yard.application_policy_set",
        "yard_application_policy",
        "yardId",
        yard_id,
        at,
    );
    event.metadata.extend([
        ("fromRevision".to_owned(), AuditValue::Null),
        ("permissionCount".to_owned(), AuditValue::Number(2)),
        ("roleCount".to_owned(), AuditValue::Number(2)),
        (
            "sourceManifestDigest".to_owned(),
            AuditValue::String(digest.to_owned()),
        ),
        ("toRevision".to_owned(), AuditValue::Number(1)),
    ]);
    event
}

pub(super) fn new_yard(name: &Slug, number: u64) -> NewWebYard {
    NewWebYard {
        id: format!("yard_{name}_{number}"),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: name.clone(),
        host_label: format!("{name}-123456789-fixture-{number}"),
        created_at_ms: number,
    }
}

pub(super) fn new_deploy(name: &Slug, number: u64, yard_id: &str) -> NewYardDeploy {
    let client = format!("clientdeploy{number:08}");
    NewYardDeploy {
        id: format!("deploy_{name}_{number}"),
        yard_id: yard_id.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        client_deploy_id: client.clone(),
        manifest_root: format!(".blobyard-yard/{yard_id}/{client}/"),
        deployment_host_label: format!("{name}-0123456789-fixture-{number}"),
        spa: true,
        clean_urls: true,
        created_at_ms: number,
    }
}

pub(super) fn event(
    action: &str,
    target_type: &str,
    key: &str,
    value: &str,
    at: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{value}_{at}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}_{at}"),
        target_type: target_type.to_owned(),
        metadata: vec![(key.to_owned(), AuditValue::String(value.to_owned()))],
        created_at_ms: at,
    }
}

pub(super) fn deployed_event(
    deploy_id: &str,
    count: u64,
    bytes: u64,
    status: &str,
    at: u64,
) -> NewAuditEvent {
    let mut event = event("yard.deployed", "yard_deploy", "deployId", deploy_id, at);
    event.metadata.extend([
        ("fileCount".to_owned(), AuditValue::Number(count)),
        ("status".to_owned(), AuditValue::String(status.to_owned())),
        ("totalBytes".to_owned(), AuditValue::Number(bytes)),
    ]);
    event
}

pub(super) fn action_event(action: &str, yard_id: &str, deploy_id: &str, at: u64) -> NewAuditEvent {
    let mut event = event(action, "yard_deploy", "deployId", deploy_id, at);
    event
        .metadata
        .push(("yardId".to_owned(), AuditValue::String(yard_id.to_owned())));
    event
}

/// Builds a deterministic user grant input for one Yard.
#[must_use]
pub fn new_grant(
    id: &str,
    yard_id: &str,
    environment_id: Option<&str>,
    expires_at_ms: Option<u64>,
    at: u64,
) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: id.to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: environment_id.map(str::to_owned),
        principal_kind: YardAccessPrincipalKind::User,
        principal_id: "user_fixture".to_owned(),
        app_roles: vec!["editor".to_owned(), "viewer".to_owned()],
        created_at_ms: at,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms,
    }
}

/// Builds the exact audit event a visibility change must persist.
#[must_use]
pub fn visibility_event(yard_id: &str, from: &str, to: &str, at: u64) -> NewAuditEvent {
    let mut event = event(
        "yard.visibility_changed",
        "yard_access_policy",
        "from",
        from,
        at,
    );
    event.metadata.extend([
        ("to".to_owned(), AuditValue::String(to.to_owned())),
        ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
    ]);
    event
}

/// Builds the exact audit event a new access grant must persist.
#[must_use]
pub fn granted_event(yard_id: &str, grant: &NewYardAccessGrant, at: u64) -> NewAuditEvent {
    let mut event = event(
        "yard.access_granted",
        "yard_access_grant",
        "grantId",
        &grant.id,
        at,
    );
    event.metadata.extend([
        (
            "environmentId".to_owned(),
            grant
                .environment_id
                .clone()
                .map_or(AuditValue::Null, AuditValue::String),
        ),
        (
            "principalKind".to_owned(),
            AuditValue::String(grant.principal_kind.as_str().to_owned()),
        ),
        ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
    ]);
    event
}

/// Builds the exact audit event an access revocation must persist.
#[must_use]
pub fn revoked_event(yard_id: &str, grant_id: &str, at: u64) -> NewAuditEvent {
    let mut event = event(
        "yard.access_revoked",
        "yard_access_grant",
        "grantId",
        grant_id,
        at,
    );
    event
        .metadata
        .push(("yardId".to_owned(), AuditValue::String(yard_id.to_owned())));
    event
}
