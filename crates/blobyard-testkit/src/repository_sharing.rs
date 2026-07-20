use blobyard_contract::{
    NewAuditEvent, NewDownloadGrant, NewShare, RepositoryError, ShareStatus, SharingRepository,
};

/// Runs deterministic public-share transitions against a populated adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn sharing_conformance(repository: &dyn SharingRepository) -> Result<(), RepositoryError> {
    let share = create_and_resolve_share(repository)?;
    exercise_download_limit(repository, &share)?;
    exercise_revocation(repository, &share)
}

fn create_and_resolve_share(
    repository: &dyn SharingRepository,
) -> Result<NewShare, RepositoryError> {
    let share = NewShare {
        id: "share_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        version_id: "upload_two".to_owned(),
        capability_hash: super::hash('e'),
        expires_at_ms: 5_000,
        maximum_downloads: Some(1),
        created_at_ms: 1_000,
    };
    let created =
        repository.create_share(&share, &share_event("share.created", &share.id, 1_000))?;
    if created.status != ShareStatus::Active || created.consumed_count != 0 {
        return Err(RepositoryError::Unavailable);
    }
    let listed = repository.list_shares(&share.workspace_id)?;
    let resolved = repository.share_by_capability(&share.capability_hash, 1_001)?;
    if listed != [created] || resolved.object.version.id != share.version_id {
        return Err(RepositoryError::Unavailable);
    }
    Ok(share)
}

fn exercise_download_limit(
    repository: &dyn SharingRepository,
    share: &NewShare,
) -> Result<(), RepositoryError> {
    let grant = NewDownloadGrant {
        version_id: share.version_id.clone(),
        capability_hash: super::hash('f'),
        expires_at_ms: 1_100,
    };
    let issued = repository.issue_share_download(
        &share.capability_hash,
        1_001,
        &grant,
        &share_event("share.download_issued", &share.id, 1_001),
    )?;
    if issued.share.status != ShareStatus::Exhausted
        || issued.share.consumed_count != 1
        || repository.issue_share_download(
            &share.capability_hash,
            1_002,
            &grant,
            &share_event("share.download_issued", &share.id, 1_002),
        ) != Err(RepositoryError::NotFound)
        || repository.share_by_capability(&share.capability_hash, 5_000)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn exercise_revocation(
    repository: &dyn SharingRepository,
    share: &NewShare,
) -> Result<(), RepositoryError> {
    if !repository.revoke_share(
        &share.id,
        &share.workspace_id,
        1_003,
        &share_event("share.revoked", &share.id, 1_003),
    )? || repository.revoke_share(
        &share.id,
        &share.workspace_id,
        1_004,
        &share_event("share.revoked", &share.id, 1_004),
    )? {
        return Err(RepositoryError::Unavailable);
    }
    let final_record = repository
        .list_shares(&share.workspace_id)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    if final_record.status != ShareStatus::Revoked
        || repository.share_by_capability(&share.capability_hash, 1_004)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

/// Builds the canonical non-secret share audit fixture.
#[must_use]
pub fn share_event(action: &str, share_id: &str, created_at_ms: u64) -> NewAuditEvent {
    super::events::capability_event(action, "share", "shareId", share_id, created_at_ms)
}
