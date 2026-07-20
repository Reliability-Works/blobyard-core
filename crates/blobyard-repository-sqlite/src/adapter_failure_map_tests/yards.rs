use super::*;

pub(super) fn assert_poisoned_yards(repository: &SqliteRepository) {
    let yard = blobyard_contract::NewWebYard {
        id: "yard_failure_map".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: slug("site"),
        host_label: "site-123456789-fixture".to_owned(),
        created_at_ms: 1_000,
    };
    let deploy = blobyard_contract::NewYardDeploy {
        id: "deploy_failure_map".to_owned(),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "client-deploy-failure-map".to_owned(),
        manifest_root: format!(
            ".blobyard-yard/{}/{}/",
            yard.id, "client-deploy-failure-map"
        ),
        deployment_host_label: "site-0123456789-fixture".to_owned(),
        spa: false,
        clean_urls: false,
        created_at_ms: 1_000,
    };
    let event = blobyard_contract::NewAuditEvent {
        id: "audit_yard_failure_map".to_owned(),
        workspace_id: yard.workspace_id.clone(),
        actor: "fixture".to_owned(),
        action: "yard.created".to_owned(),
        request_id: "request_yard_failure_map".to_owned(),
        target_type: "web_yard".to_owned(),
        metadata: vec![(
            "yardId".to_owned(),
            blobyard_contract::AuditValue::String(yard.id.clone()),
        )],
        created_at_ms: 1_000,
    };
    unavailable(repository.start_yard_deploy(&yard, &deploy, &event));
    unavailable(repository.list_web_yards(&yard.project_id));
    unavailable(repository.web_yard_by_id(&yard.id));
    unavailable(repository.list_yard_deploys(&yard.id));
    unavailable(repository.yard_deploy_by_id(&deploy.id));
    unavailable(repository.finalise_yard_deploy(&deploy.id, &[], 1_001, &event));
    unavailable(repository.fail_yard_deploy(&deploy.id, "FAILED", "failed", 1_001));
    unavailable(repository.rollback_web_yard(&yard.id, Some(&deploy.id), 1_001, &event));
    unavailable(repository.delete_web_yard(&yard.id, 1_001, &event));
    unavailable(repository.yard_file_by_host(&yard.host_label, ""));
}
