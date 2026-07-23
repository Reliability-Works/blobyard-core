use super::{
    success,
    transaction_edges::support::{created, deploy, repository, yard},
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardAccessGrant, RepositoryError, WebYardRepository,
    YardAccessPrincipalKind, YardVisibility,
};

#[test]
fn access_mutations_conceal_unknown_and_deleted_yards() {
    let (_temporary, repository, _version, _size) = repository();
    let candidate = yard("docs", 1);
    success(repository.start_yard_deploy(
        &candidate,
        &deploy(&candidate, 1, false),
        &created(&candidate.id, 1),
    ));
    success(repository.delete_web_yard(&candidate.id, 2, &deleted_event(&candidate.id, 2)));
    for yard_id in ["yard_unknown", candidate.id.as_str()] {
        assert_eq!(
            repository.set_yard_visibility(
                yard_id,
                YardVisibility::Owner,
                3,
                &visibility_event(yard_id, "public", "owner", 3),
            ),
            Err(RepositoryError::NotFound)
        );
        assert_eq!(
            repository.insert_yard_access_grant(
                &grant_input("grant_concealed", yard_id, &["viewer"], 3),
                &granted_event(&grant_input("grant_concealed", yard_id, &["viewer"], 3), 3),
            ),
            Err(RepositoryError::NotFound)
        );
        assert_eq!(
            repository.revoke_yard_access_grant(
                yard_id,
                "grant_concealed",
                3,
                &revoked_event(yard_id, "grant_concealed", 3),
            ),
            Err(RepositoryError::NotFound)
        );
    }
}

#[test]
fn grants_validate_roles_and_conceal_foreign_yards() {
    let (_temporary, repository, _version, _size) = repository();
    let docs = yard("docs", 1);
    success(repository.start_yard_deploy(
        &docs,
        &deploy(&docs, 1, false),
        &created(&docs.id, 1),
    ));
    let blog = yard("blog", 2);
    success(repository.start_yard_deploy(
        &blog,
        &deploy(&blog, 2, false),
        &created(&blog.id, 2),
    ));
    let oversized = "role".repeat(17);
    let many = (0..17).map(|index| format!("role{index}")).collect::<Vec<_>>();
    let invalid_roles: [&[&str]; 3] = [&["viewer", "viewer"], &[oversized.as_str()], &["bad\u{1}"]];
    for roles in invalid_roles {
        let grant = grant_input("grant_invalid", &docs.id, roles, 3);
        assert_eq!(
            repository.insert_yard_access_grant(&grant, &granted_event(&grant, 3)),
            Err(RepositoryError::InvalidInput)
        );
    }
    let mut crowded = grant_input("grant_invalid", &docs.id, &[], 3);
    crowded.app_roles = many;
    assert_eq!(
        repository.insert_yard_access_grant(&crowded, &granted_event(&crowded, 3)),
        Err(RepositoryError::InvalidInput)
    );
    let mut unnamed = grant_input("grant_invalid", &docs.id, &["viewer"], 3);
    unnamed.id.clear();
    assert_eq!(
        repository.insert_yard_access_grant(&unnamed, &granted_event(&unnamed, 3)),
        Err(RepositoryError::InvalidInput)
    );
    let granted = grant_input("grant_docs", &docs.id, &["viewer"], 3);
    success(repository.insert_yard_access_grant(&granted, &granted_event(&granted, 3)));
    assert_eq!(
        repository.revoke_yard_access_grant(
            &blog.id,
            "grant_docs",
            4,
            &revoked_event(&blog.id, "grant_docs", 4),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn visibility_round_trips_and_rejects_mismatched_audit_events() {
    let (_temporary, repository, _version, _size) = repository();
    let candidate = yard("docs", 1);
    success(repository.start_yard_deploy(
        &candidate,
        &deploy(&candidate, 1, false),
        &created(&candidate.id, 1),
    ));
    assert_eq!(
        repository.set_yard_visibility(
            &candidate.id,
            YardVisibility::Workspace,
            2,
            &visibility_event(&candidate.id, "owner", "workspace", 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let policy = success(repository.set_yard_visibility(
        &candidate.id,
        YardVisibility::Workspace,
        2,
        &visibility_event(&candidate.id, "public", "workspace", 2),
    ));
    assert_eq!(policy.visibility, YardVisibility::Workspace);
    assert_eq!(policy.updated_by_principal, "fixture");
    let read = success(repository.get_yard_access_policy(&candidate.id));
    assert_eq!(read, Some(policy));
}

fn grant_input(id: &str, yard_id: &str, roles: &[&str], at: u64) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: id.to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::User,
        principal_id: "user_fixture".to_owned(),
        app_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        created_at_ms: at,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    }
}

fn access_event(
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    at: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{at}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}_{at}"),
        target_type: target_type.to_owned(),
        metadata,
        created_at_ms: at,
    }
}

fn visibility_event(yard_id: &str, from: &str, to: &str, at: u64) -> NewAuditEvent {
    access_event(
        "yard.visibility_changed",
        "yard_access_policy",
        vec![
            ("from".to_owned(), AuditValue::String(from.to_owned())),
            ("to".to_owned(), AuditValue::String(to.to_owned())),
            ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
        ],
        at,
    )
}

fn granted_event(grant: &NewYardAccessGrant, at: u64) -> NewAuditEvent {
    access_event(
        "yard.access_granted",
        "yard_access_grant",
        vec![
            (
                "environmentId".to_owned(),
                grant
                    .environment_id
                    .clone()
                    .map_or(AuditValue::Null, AuditValue::String),
            ),
            ("grantId".to_owned(), AuditValue::String(grant.id.clone())),
            (
                "principalKind".to_owned(),
                AuditValue::String(grant.principal_kind.as_str().to_owned()),
            ),
            (
                "yardId".to_owned(),
                AuditValue::String(grant.yard_id.clone()),
            ),
        ],
        at,
    )
}

fn revoked_event(yard_id: &str, grant_id: &str, at: u64) -> NewAuditEvent {
    access_event(
        "yard.access_revoked",
        "yard_access_grant",
        vec![
            ("grantId".to_owned(), AuditValue::String(grant_id.to_owned())),
            ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
        ],
        at,
    )
}

fn deleted_event(yard_id: &str, at: u64) -> NewAuditEvent {
    access_event(
        "yard.deleted",
        "web_yard",
        vec![("yardId".to_owned(), AuditValue::String(yard_id.to_owned()))],
        at,
    )
}
