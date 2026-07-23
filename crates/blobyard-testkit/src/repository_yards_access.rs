use super::YardConformanceRepository;
use super::delivery::assert_delivery;
use super::fixtures::{granted_event, new_grant, revoked_event, visibility_event};
use blobyard_contract::{
    RepositoryError, RevocableStatus, YardAccessGrantRecord, YardStartRecord, YardVisibility,
};

pub(super) fn assert_access_controls(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
) -> Result<(), RepositoryError> {
    assert_default_public(repository, &first.yard.id)?;
    assert_private_concealment(repository, first)?;
    let grants = assert_grant_lifecycle(repository, &first.yard.id)?;
    assert_revocation(repository, &first.yard.id, &grants)?;
    assert_restored_delivery(repository, first, version_id)
}

fn assert_default_public(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
) -> Result<(), RepositoryError> {
    if repository.get_yard_access_policy(yard_id)?.is_some() {
        return Err(RepositoryError::Unavailable);
    }
    if !repository.list_yard_access_grants(yard_id, 5)?.is_empty() {
        return Err(RepositoryError::Unavailable);
    }
    if !repository
        .list_yard_access_grants("yard_unknown", 5)?
        .is_empty()
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_private_concealment(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    let policy = repository.set_yard_visibility(
        &first.yard.id,
        YardVisibility::Owner,
        6,
        &visibility_event(&first.yard.id, "public", "owner", 6),
    )?;
    let updated = policy.yard_id == first.yard.id
        && policy.visibility == YardVisibility::Owner
        && policy.updated_at_ms == 6
        && !policy.updated_by_principal.is_empty();
    if !updated {
        return Err(RepositoryError::Unavailable);
    }
    if repository.yard_file_by_host(&first.yard.host_label, "asset.js")
        != Err(RepositoryError::NotFound)
        || repository.yard_file_by_host(&first.deploy.deployment_host_label, "asset.js")
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_grant_lifecycle(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
) -> Result<[YardAccessGrantRecord; 2], RepositoryError> {
    let environment_id = repository
        .list_yard_environments(yard_id)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?
        .id;
    let open = new_grant("grant_docs_open", yard_id, None, None, 7);
    let open_record =
        repository.insert_yard_access_grant(&open, &granted_event(yard_id, &open, 7))?;
    let open_matches = open_record.id == open.id
        && open_record.yard_id == yard_id
        && open_record.environment_id.is_none()
        && open_record.principal_kind == open.principal_kind
        && open_record.principal_id == open.principal_id
        && open_record.app_roles == open.app_roles
        && open_record.status == RevocableStatus::Active
        && open_record.created_at_ms == 7
        && open_record.expires_at_ms.is_none()
        && open_record.revoked_at_ms.is_none();
    if !open_matches {
        return Err(RepositoryError::Unavailable);
    }
    let scoped = new_grant(
        "grant_docs_scoped",
        yard_id,
        Some(&environment_id),
        Some(1_000),
        8,
    );
    let scoped_record =
        repository.insert_yard_access_grant(&scoped, &granted_event(yard_id, &scoped, 8))?;
    if scoped_record.environment_id.as_deref() != Some(environment_id.as_str())
        || scoped_record.expires_at_ms != Some(1_000)
    {
        return Err(RepositoryError::Unavailable);
    }
    assert_grant_rejections(repository, yard_id)?;
    let listed = repository.list_yard_access_grants(yard_id, 9)?;
    if listed != [scoped_record.clone(), open_record.clone()] {
        return Err(RepositoryError::Unavailable);
    }
    if repository.list_yard_access_grants(yard_id, 1_000)? != [open_record.clone()] {
        return Err(RepositoryError::Unavailable);
    }
    Ok([open_record, scoped_record])
}

fn assert_grant_rejections(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
) -> Result<(), RepositoryError> {
    let foreign = new_grant(
        "grant_docs_foreign",
        yard_id,
        Some("yardenv_unknown"),
        None,
        9,
    );
    let expired = new_grant("grant_docs_expired", yard_id, None, Some(3), 9);
    if repository.insert_yard_access_grant(&foreign, &granted_event(yard_id, &foreign, 9))
        != Err(RepositoryError::InvalidInput)
        || repository.insert_yard_access_grant(&expired, &granted_event(yard_id, &expired, 9))
            != Err(RepositoryError::InvalidInput)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_revocation(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
    grants: &[YardAccessGrantRecord; 2],
) -> Result<(), RepositoryError> {
    if repository.revoke_yard_access_grant(
        yard_id,
        "grant_missing",
        13,
        &revoked_event(yard_id, "grant_missing", 13),
    ) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    if !repository.revoke_yard_access_grant(
        yard_id,
        &grants[0].id,
        14,
        &revoked_event(yard_id, &grants[0].id, 14),
    )? {
        return Err(RepositoryError::Unavailable);
    }
    if repository.revoke_yard_access_grant(
        yard_id,
        &grants[0].id,
        15,
        &revoked_event(yard_id, &grants[0].id, 15),
    )? {
        return Err(RepositoryError::Unavailable);
    }
    if repository.list_yard_access_grants(yard_id, 16)? != [grants[1].clone()] {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_restored_delivery(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let restored = repository.set_yard_visibility(
        &first.yard.id,
        YardVisibility::Public,
        17,
        &visibility_event(&first.yard.id, "owner", "public", 17),
    )?;
    if restored.visibility != YardVisibility::Public {
        return Err(RepositoryError::Unavailable);
    }
    assert_delivery(repository, &first.yard.host_label, version_id)?;
    assert_delivery(repository, &first.deploy.deployment_host_label, version_id)
}
