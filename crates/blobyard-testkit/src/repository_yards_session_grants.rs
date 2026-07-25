use super::YardConformanceRepository;
use super::session_fixtures::set_visibility;
use super::session_groups::{
    add_member, group_grant, group_record, require_delivery, require_denial,
};
use blobyard_contract::{
    AuditValue, NewYardAccessGrant, RepositoryError, YardAccessPrincipalKind, YardSessionRecord,
    YardStartRecord, YardVisibility,
};

pub(super) fn select_with_independent_empty_role_grant(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    let empty = group_grant(
        "grant_yard_group_empty",
        &first.yard.id,
        None,
        Vec::new(),
        103,
        None,
    );
    insert_grant(repository, &empty)?;
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Selected,
        104,
    )?;
    repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", 104)?;
    let second = group_grant(
        "grant_yard_group_second",
        &first.yard.id,
        None,
        vec!["viewer".to_owned()],
        104,
        None,
    );
    insert_grant(repository, &second)?;
    repository
        .evaluate_yard_admission(&first.yard.host_label, "user_fixture", 104)
        .map(|_admission| ())
}

pub(super) fn assert_grant_transitions(
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
        121,
    )?;
    require_delivery(repository, first, version_id, session, 121)?;
    assert_direct_grant_preserves_access(repository, first, version_id, session)?;
    assert_revoked_group_grants_deny(repository, first, session)?;
    assert_environment_and_expiry(repository, first, version_id, session)
}

fn assert_direct_grant_preserves_access(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    let direct = direct_grant(&first.yard.id, 122);
    insert_grant(repository, &direct)?;
    let group = group_record();
    repository.remove_workspace_group_member(
        &group.workspace_id,
        &group.id,
        "user_fixture",
        &crate::group_event(
            "audit_yard_group_removed",
            "group.member_removed",
            &group,
            123,
            [("userId", AuditValue::String("user_fixture".to_owned()))],
        ),
    )?;
    require_delivery(repository, first, version_id, session, 123)?;
    revoke_grant(repository, &first.yard.id, &direct.id, 124)?;
    require_denial(repository, first, session, 124)?;
    add_member(repository, &group, 125, "audit_yard_group_readded")?;
    require_delivery(repository, first, version_id, session, 125)
}

fn assert_revoked_group_grants_deny(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    revoke_grant(repository, &first.yard.id, "grant_yard_group_empty", 126)?;
    revoke_grant(repository, &first.yard.id, "grant_yard_group_second", 127)?;
    require_denial(repository, first, session, 127)
}

fn assert_environment_and_expiry(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    let scoped = group_grant(
        "grant_yard_group_environment",
        &first.yard.id,
        Some(session.environment_id.clone()),
        Vec::new(),
        128,
        None,
    );
    insert_grant(repository, &scoped)?;
    require_delivery(repository, first, version_id, session, 128)?;
    let expiring = group_grant(
        "grant_yard_group_expiring",
        &first.yard.id,
        None,
        vec!["viewer".to_owned()],
        129,
        Some(130),
    );
    insert_grant(repository, &expiring)?;
    revoke_grant(repository, &first.yard.id, &scoped.id, 130)?;
    require_denial(repository, first, session, 130)?;
    let restored = group_grant(
        "grant_yard_group_restored",
        &first.yard.id,
        None,
        Vec::new(),
        131,
        None,
    );
    insert_grant(repository, &restored).map(|_record| ())
}

fn direct_grant(yard_id: &str, created_at_ms: u64) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: "grant_yard_direct".to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::User,
        principal_id: "user_fixture".to_owned(),
        app_roles: Vec::new(),
        created_at_ms,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    }
}

fn insert_grant(
    repository: &dyn YardConformanceRepository,
    grant: &NewYardAccessGrant,
) -> Result<blobyard_contract::YardAccessGrantRecord, RepositoryError> {
    repository.insert_yard_access_grant(
        grant,
        &super::fixtures::granted_event(&grant.yard_id, grant, grant.created_at_ms),
    )
}

fn revoke_grant(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
    grant_id: &str,
    at_ms: u64,
) -> Result<(), RepositoryError> {
    repository
        .revoke_yard_access_grant(
            yard_id,
            grant_id,
            at_ms,
            &super::fixtures::revoked_event(yard_id, grant_id, at_ms),
        )
        .map(|_revoked| ())
}
