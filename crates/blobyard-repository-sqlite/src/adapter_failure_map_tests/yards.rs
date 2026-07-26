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
    unavailable(repository.list_yard_environments(&yard.id));
    unavailable(repository.yard_deploy_by_id(&deploy.id));
    unavailable(repository.finalise_yard_deploy(&deploy.id, &[], 1_001, &event));
    unavailable(repository.fail_yard_deploy(&deploy.id, "FAILED", "failed", 1_001));
    unavailable(repository.rollback_web_yard(&yard.id, Some(&deploy.id), 1_001, &event));
    unavailable(repository.delete_web_yard(&yard.id, 1_001, &event));
    unavailable(repository.yard_file_by_host(&yard.host_label, "", None, 0));
    assert_poisoned_yard_sessions(repository, &yard, &event);
    assert_poisoned_yard_access(repository, &yard.id, &event);
}

fn assert_poisoned_yard_sessions(
    repository: &SqliteRepository,
    yard: &blobyard_contract::NewWebYard,
    event: &blobyard_contract::NewAuditEvent,
) {
    unavailable(repository.evaluate_yard_admission(&yard.host_label, "user_failure_map", 1_000));
    unavailable(
        repository.issue_yard_exchange_code(&blobyard_contract::NewYardContinuation {
            id: "continuation_failure_map".to_owned(),
            continuation_hash: checksum('1'),
            code_hash: checksum('2'),
            yard_id: yard.id.clone(),
            environment_id: "environment_failure_map".to_owned(),
            host_label: yard.host_label.clone(),
            user_id: "user_failure_map".to_owned(),
            return_path: "/".to_owned(),
            created_at_ms: 1_000,
            expires_at_ms: 301_000,
        }),
    );
    unavailable(repository.list_yard_sessions(&yard.id));
    unavailable(repository.revoke_yard_session(&yard.id, "session_failure_map", 1_001, event));
    unavailable(repository.revoke_yard_session_by_token(&checksum('3'), &yard.host_label, 1_001));
    unavailable(repository.purge_yard_session_history(1_001));
}

fn assert_poisoned_yard_access(
    repository: &SqliteRepository,
    yard_id: &str,
    event: &blobyard_contract::NewAuditEvent,
) {
    let grant = blobyard_contract::NewYardAccessGrant {
        id: "grant_failure_map".to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: None,
        principal_kind: blobyard_contract::YardAccessPrincipalKind::User,
        principal_id: "user_failure_map".to_owned(),
        app_roles: vec!["viewer".to_owned()],
        created_at_ms: 1_000,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    unavailable(repository.get_yard_access_policy(yard_id));
    unavailable(repository.set_yard_visibility(
        yard_id,
        blobyard_contract::YardVisibility::Owner,
        1_001,
        event,
    ));
    unavailable(repository.insert_yard_access_grant(&grant, event));
    unavailable(repository.revoke_yard_access_grant(yard_id, &grant.id, 1_001, event));
    unavailable(repository.list_yard_access_grants(yard_id, 1_001));
}
