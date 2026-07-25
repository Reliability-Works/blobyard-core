use super::YardConformanceRepository;
use super::session_fixtures::set_visibility;
use blobyard_contract::{
    AuditValue, NewYardAccessGrant, RepositoryError, WorkspaceGroupMemberRecord,
    WorkspaceGroupRecord, WorkspaceGroupStatus, YardAccessPrincipalKind, YardSessionRecord,
    YardStartRecord, YardVisibility,
};

pub(super) fn create_group_access(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
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
    let grant = NewYardAccessGrant {
        id: "grant_yard_group".to_owned(),
        yard_id: first.yard.id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group.id,
        app_roles: vec!["viewer".to_owned()],
        created_at_ms: 104,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    repository
        .insert_yard_access_grant(
            &grant,
            &super::fixtures::granted_event(&first.yard.id, &grant, 104),
        )
        .map(|_grant| ())
}

pub(super) fn assert_live_policy(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Selected,
        130,
    )?;
    require_delivery(repository, first, version_id, session, 130)?;
    assert_membership_is_live(repository, first, version_id, session)?;
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
        133,
    )?;
    require_delivery(repository, first, version_id, session, 133)?;
    let group = group_record();
    repository.deactivate_workspace_group(
        &group.workspace_id,
        &group.id,
        134,
        &crate::group_event(
            "audit_yard_group_deactivated",
            "group.deactivated",
            &group,
            134,
            [],
        ),
    )?;
    require_denial(repository, first, session, 134)
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
        135,
    )?;
    require_delivery(repository, first, version_id, session, 135)?;
    set_visibility(
        repository,
        &first.yard.id,
        "workspace",
        YardVisibility::Owner,
        136,
    )?;
    require_denial(repository, first, session, 136)?;
    set_visibility(
        repository,
        &first.yard.id,
        "owner",
        YardVisibility::Public,
        137,
    )?;
    let public =
        repository.yard_file_by_host(&first.yard.host_label, "asset.js", Some("malformed"), 137)?;
    if public.object.version.id != version_id {
        return Err(RepositoryError::Unavailable);
    }
    set_visibility(
        repository,
        &first.yard.id,
        "public",
        YardVisibility::AnyAuthenticated,
        138,
    )
}

fn assert_membership_is_live(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    let group = group_record();
    repository.remove_workspace_group_member(
        &group.workspace_id,
        &group.id,
        "user_fixture",
        &crate::group_event(
            "audit_yard_group_removed",
            "group.member_removed",
            &group,
            131,
            [("userId", AuditValue::String("user_fixture".to_owned()))],
        ),
    )?;
    require_denial(repository, first, session, 131)?;
    add_member(repository, &group, 132, "audit_yard_group_readded")?;
    require_delivery(repository, first, version_id, session, 132)
}

fn add_member(
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

fn group_record() -> WorkspaceGroupRecord {
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

fn require_delivery(
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

fn require_denial(
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
