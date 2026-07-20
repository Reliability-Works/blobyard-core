use blobyard_contract::{AuditValue, NewAuditEvent, NewWebYard, NewYardDeploy};
use blobyard_core::Slug;

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
