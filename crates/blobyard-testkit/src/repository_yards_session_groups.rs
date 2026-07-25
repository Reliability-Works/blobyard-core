use super::YardConformanceRepository;
use super::session_fixtures::set_visibility;
use blobyard_contract::{
    AuditValue, NewYardAccessGrant, RepositoryError, WorkspaceGroupMemberRecord,
    WorkspaceGroupRecord, WorkspaceGroupStatus, WorkspaceRecord, YardAccessPrincipalKind,
    YardSessionRecord, YardStartRecord, YardVisibility,
};

pub(super) fn create_group_access(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    create_foreign_group(repository, first)?;
    let group = group_record();
    repository.create_workspace_group(
        &group,
        &crate::group_event(
            "audit_yard_group",
            "group.created",
            &group,
            102,
            [("name", AuditValue::String(group.name.clone()))],
        ),
    )?;
    add_member(repository, &group, 103, "audit_yard_group_member")?;
    for (id, roles, at_ms) in [
        ("grant_yard_group_empty", Vec::new(), 103),
        ("grant_yard_group_second", vec!["viewer".to_owned()], 104),
    ] {
        let grant = group_grant(id, &first.yard.id, None, roles, at_ms, None);
        repository.insert_yard_access_grant(
            &grant,
            &super::fixtures::granted_event(&first.yard.id, &grant, at_ms),
        )?;
    }
    Ok(())
}

fn create_foreign_group(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    let workspace = WorkspaceRecord {
        id: "workspace_yard_foreign".to_owned(),
        name: "Foreign Yard workspace".to_owned(),
        slug: first.yard.name.clone(),
    };
    repository.create_workspace(&workspace)?;
    let user = crate::local_user(&workspace.id, "user_yard_foreign", None, 100);
    repository.create_local_user(
        &user,
        &crate::login_key("userkey_yard_foreign", &user.id, '6', 100),
        &crate::local_user_event("audit_user_yard_foreign", &user, "user.created", 100),
    )?;
    let group = WorkspaceGroupRecord {
        id: "group_ffffffffffffffffffffffffffffffff".to_owned(),
        workspace_id: workspace.id,
        name: group_record().name,
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 101,
        deactivated_at_ms: None,
    };
    repository.create_workspace_group(
        &group,
        &crate::group_event(
            "audit_yard_group_foreign",
            "group.created",
            &group,
            101,
            [("name", AuditValue::String(group.name.clone()))],
        ),
    )?;
    add_foreign_member(repository, &group)?;
    let grant = NewYardAccessGrant {
        id: "grant_yard_foreign_group".to_owned(),
        yard_id: first.yard.id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group.id,
        app_roles: Vec::new(),
        created_at_ms: 101,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    if repository.insert_yard_access_grant(
        &grant,
        &super::fixtures::granted_event(&first.yard.id, &grant, 101),
    ) == Err(RepositoryError::NotFound)
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn add_foreign_member(
    repository: &dyn YardConformanceRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository.add_workspace_group_member(
        &WorkspaceGroupMemberRecord {
            group_id: group.id.clone(),
            workspace_id: group.workspace_id.clone(),
            user_id: "user_yard_foreign".to_owned(),
            added_at_ms: 101,
        },
        &crate::group_event(
            "audit_yard_group_foreign_member",
            "group.member_added",
            group,
            101,
            [("userId", AuditValue::String("user_yard_foreign".to_owned()))],
        ),
    )
}

pub(super) fn assert_live_policy(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    super::session_grants::assert_grant_transitions(repository, first, version_id, session)?;
    assert_authenticated_link_and_deactivation(repository, first, version_id, session)?;
    assert_workspace_owner_and_public(repository, first, version_id, session)
}

fn assert_authenticated_link_and_deactivation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AuthenticatedLink,
        132,
    )?;
    require_delivery(repository, first, version_id, session, 132)?;
    let group = group_record();
    repository.deactivate_workspace_group(
        &group.workspace_id,
        &group.id,
        133,
        &crate::group_event(
            "audit_yard_group_deactivated",
            "group.deactivated",
            &group,
            133,
            [],
        ),
    )?;
    require_denial(repository, first, session, 133)
}

fn assert_workspace_owner_and_public(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    set_visibility(
        repository,
        &first.yard.id,
        "authenticated-link",
        YardVisibility::Workspace,
        134,
    )?;
    require_delivery(repository, first, version_id, session, 134)?;
    set_visibility(
        repository,
        &first.yard.id,
        "workspace",
        YardVisibility::Owner,
        135,
    )?;
    require_denial(repository, first, session, 135)?;
    set_visibility(
        repository,
        &first.yard.id,
        "owner",
        YardVisibility::Public,
        136,
    )?;
    let public =
        repository.yard_file_by_host(&first.yard.host_label, "asset.js", Some("malformed"), 136)?;
    if public.object.version.id != version_id {
        return Err(RepositoryError::Unavailable);
    }
    set_visibility(
        repository,
        &first.yard.id,
        "public",
        YardVisibility::AnyAuthenticated,
        137,
    )
}

pub(super) fn add_member(
    repository: &dyn YardConformanceRepository,
    group: &WorkspaceGroupRecord,
    at_ms: u64,
    event_id: &str,
) -> Result<(), RepositoryError> {
    repository.add_workspace_group_member(
        &WorkspaceGroupMemberRecord {
            group_id: group.id.clone(),
            workspace_id: group.workspace_id.clone(),
            user_id: "user_fixture".to_owned(),
            added_at_ms: at_ms,
        },
        &crate::group_event(
            event_id,
            "group.member_added",
            group,
            at_ms,
            [("userId", AuditValue::String("user_fixture".to_owned()))],
        ),
    )
}

pub(super) fn group_record() -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: "group_00000000000000000000000000000003".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Yard readers".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 102,
        deactivated_at_ms: None,
    }
}

pub(super) fn require_delivery(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
    at_ms: u64,
) -> Result<(), RepositoryError> {
    let target = repository.yard_file_by_host(
        &first.yard.host_label,
        "asset.js",
        Some(&session.token_hash),
        at_ms,
    )?;
    if target.object.version.id == version_id {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

pub(super) fn require_denial(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    session: &YardSessionRecord,
    at_ms: u64,
) -> Result<(), RepositoryError> {
    if repository.yard_file_by_host(
        &first.yard.host_label,
        "asset.js",
        Some(&session.token_hash),
        at_ms,
    ) == Err(RepositoryError::NotFound)
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

pub(super) fn group_grant(
    id: &str,
    yard_id: &str,
    environment_id: Option<String>,
    app_roles: Vec<String>,
    created_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: id.to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group_record().id,
        app_roles,
        created_at_ms,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms,
    }
}
