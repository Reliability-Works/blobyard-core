use blobyard_contract::{
    MetadataRepository, NewDownloadGrant, NewObjectVersion, ObjectSource, ProjectRecord,
    RepositoryError, ReservationState, TransferRepository, UploadState, WorkspaceRecord,
};
#[path = "repository_events.rs"]
mod events;
#[path = "repository_fixtures.rs"]
mod fixtures;
#[path = "repository_inboxes.rs"]
mod inboxes;
#[path = "repository_multipart.rs"]
mod multipart;
#[path = "repository_previews.rs"]
mod previews;
#[path = "repository_sharing.rs"]
mod sharing;
#[path = "repository_yards.rs"]
mod yards;
use fixtures::{NamespaceFixtures, ValidatedNamespaceFixtures, hash, hello_checksum, upload};
pub use inboxes::{InboxConformanceRepository, inbox_conformance, inbox_event, inbox_upload_event};
pub use previews::{PreviewConformanceRepository, preview_conformance, preview_event};
pub use sharing::{share_event, sharing_conformance};
pub use yards::{YardConformanceFixture, YardConformanceRepository, yard_conformance, yard_event};

/// Runs the deterministic metadata repository contract against one empty adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn repository_conformance(repository: &dyn MetadataRepository) -> Result<(), RepositoryError> {
    repository_conformance_with(repository, &NamespaceFixtures::valid())
}

fn repository_conformance_with(
    repository: &dyn MetadataRepository,
    fixtures: &NamespaceFixtures,
) -> Result<(), RepositoryError> {
    fixtures
        .validate()
        .and_then(|fixtures| repository_conformance_validated(repository, fixtures))
}

fn repository_conformance_validated(
    repository: &dyn MetadataRepository,
    fixtures: ValidatedNamespaceFixtures,
) -> Result<(), RepositoryError> {
    if repository.schema_version()? != 16 {
        return Err(RepositoryError::SchemaTooNew);
    }
    let project = namespace_conformance(repository, fixtures)?;
    object_version_conformance(repository, project)
}

/// Runs deterministic durable transfer transitions against a populated adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn transfer_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    upload_lifecycle_conformance(repository, project_id)?;
    version_listing_conformance(repository, project_id)?;
    download_grant_conformance(repository, project_id)?;
    abort_conformance(repository, project_id)?;
    multipart::conformance(repository, project_id)
}

fn download_grant_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let object = repository
        .list_stored_objects(project_id, Some("artifacts/build.zip"), false)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    let grant = NewDownloadGrant {
        version_id: object.version.id,
        capability_hash: hash('5'),
        expires_at_ms: 4_000,
    };
    repository.issue_download(&grant)?;
    let resolved = repository.download_by_capability(&grant.capability_hash, 3_999)?;
    if resolved.version.version != 2
        || repository.download_by_capability(&grant.capability_hash, 4_000)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn upload_lifecycle_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let first = upload("upload_one", project_id, "artifacts/build.zip", '1');
    let reserved = repository.reserve_upload(&first)?;
    if reserved.version.version != 1
        || reserved.state != ReservationState::Requested
        || reserved.version.source != first.source
        || reserved.version.git_repository != first.git_repository
        || reserved.version.git_commit != first.git_commit
        || reserved.version.git_branch != first.git_branch
    {
        return Err(RepositoryError::Unavailable);
    }
    assert_equal(
        &repository.upload_by_capability(&first.capability_hash, 999)?,
        &reserved,
    )?;
    if repository.upload_by_capability(&first.capability_hash, 1_000)
        != Err(RepositoryError::NotFound)
        || repository.record_uploaded_bytes(&first.id, 4, hello_checksum())
            != Err(RepositoryError::InvalidInput)
        || repository.record_uploaded_bytes(&first.id, 5, &hash('f'))
            != Err(RepositoryError::InvalidInput)
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.renew_upload(&first.id, 2_000)?;
    if repository
        .upload_by_capability(&first.capability_hash, 1_999)?
        .expires_at_ms
        != 2_000
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.record_uploaded_bytes(&first.id, 5, hello_checksum())?;
    if repository.upload_by_capability(&first.capability_hash, 1) != Err(RepositoryError::NotFound)
        || repository.record_uploaded_bytes(&first.id, 5, hello_checksum())
            != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    let completed = repository.complete_upload(&first.id)?;
    if completed.state != UploadState::Complete || completed.version != 1 {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn version_listing_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let second = upload("upload_two", project_id, "artifacts/build.zip", '2');
    let second_record = repository.reserve_upload(&second)?;
    if second_record.version.version != 2 {
        return Err(RepositoryError::Unavailable);
    }
    repository.record_uploaded_bytes(&second.id, 5, hello_checksum())?;
    repository.complete_upload(&second.id)?;
    let latest = repository.list_stored_objects(project_id, Some("artifacts/"), false)?;
    if latest.len() != 1 || latest[0].version.version != 2 {
        return Err(RepositoryError::Unavailable);
    }
    let all = repository.list_stored_objects(project_id, None, true)?;
    if all.len() != 2 || all[0].version.version != 1 || all[1].version.version != 2 {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn abort_conformance(
    repository: &dyn TransferRepository,
    project_id: &str,
) -> Result<(), RepositoryError> {
    let requested = upload("upload_abort_requested", project_id, "abort/requested", '3');
    repository.reserve_upload(&requested)?;
    let prior = repository.abort_upload(&requested.id)?;
    if prior.state != ReservationState::Requested
        || repository.upload_by_id(&requested.id)?.state != ReservationState::Aborted
        || repository.abort_upload(&requested.id) != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    let uploaded = upload("upload_abort_uploaded", project_id, "abort/uploaded", '4');
    repository.reserve_upload(&uploaded)?;
    repository.record_uploaded_bytes(&uploaded.id, 5, hello_checksum())?;
    if repository.abort_upload(&uploaded.id)?.state != ReservationState::Uploaded
        || repository.upload_by_id(&uploaded.id)?.state != ReservationState::Aborted
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn namespace_conformance(
    repository: &dyn MetadataRepository,
    fixtures: ValidatedNamespaceFixtures,
) -> Result<ProjectRecord, RepositoryError> {
    let workspace = WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Fixture workspace".to_owned(),
        slug: fixtures.workspace,
    };
    repository.create_workspace(&workspace)?;
    assert_equal(&repository.list_workspaces()?, &vec![workspace.clone()])?;
    assert_equal(&repository.workspace_by_slug(&workspace.slug)?, &workspace)?;
    if repository.create_workspace(&workspace) != Err(RepositoryError::Conflict) {
        return Err(RepositoryError::Unavailable);
    }
    let renamed = WorkspaceRecord {
        name: "Renamed workspace".to_owned(),
        slug: fixtures.renamed_workspace,
        ..workspace.clone()
    };
    repository.rename_workspace(
        &renamed,
        &crate::workspace_renamed_event(&workspace.id, workspace.slug.as_str(), 1),
    )?;
    assert_equal(&repository.list_workspaces()?, &vec![renamed.clone()])?;
    assert_equal(&repository.workspace_by_slug(&renamed.slug)?, &renamed)?;
    if repository.workspace_by_slug(&workspace.slug) != Err(RepositoryError::NotFound) {
        return Err(RepositoryError::Unavailable);
    }
    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: renamed.id,
        name: "Fixture project".to_owned(),
        slug: fixtures.project,
    };
    repository.create_project(&project)?;
    assert_equal(
        &repository.list_projects(&project.workspace_id)?,
        &vec![project.clone()],
    )?;
    assert_equal(
        &repository.project_by_slug(&project.workspace_id, &project.slug)?,
        &project,
    )?;
    Ok(project)
}

fn object_version_conformance(
    repository: &dyn MetadataRepository,
    project: ProjectRecord,
) -> Result<(), RepositoryError> {
    let pending = NewObjectVersion {
        id: "version_pending".to_owned(),
        project_id: project.id,
        object_path: "reports/example.txt".to_owned(),
        version: 1,
        storage_key: "objects/version_pending".to_owned(),
        source: ObjectSource::Web,
        git_repository: Some("Reliability-Works/blobyard-core".to_owned()),
        git_commit: Some("0123456789abcdef".to_owned()),
        git_branch: Some("main".to_owned()),
    };
    repository.reserve_object_version(&pending)?;
    if repository.reserve_object_version(&pending) != Err(RepositoryError::Conflict) {
        return Err(RepositoryError::Unavailable);
    }
    repository.complete_object_version(&pending.id, 5, hello_checksum())?;
    let complete = repository.object_version(&pending.id)?;
    if complete.state != UploadState::Complete
        || complete.size != Some(5)
        || complete.checksum.as_deref() != Some(hello_checksum())
        || complete.source != pending.source
        || complete.git_repository != pending.git_repository
        || complete.git_commit != pending.git_commit
        || complete.git_branch != pending.git_branch
    {
        return Err(RepositoryError::Unavailable);
    }
    if repository.abort_object_version(&pending.id) != Err(RepositoryError::Conflict) {
        return Err(RepositoryError::Unavailable);
    }
    let aborted = NewObjectVersion {
        id: "version_aborted".to_owned(),
        object_path: "reports/aborted.txt".to_owned(),
        version: 1,
        storage_key: "objects/version_aborted".to_owned(),
        ..pending
    };
    repository.reserve_object_version(&aborted)?;
    repository.abort_object_version(&aborted.id)?;
    if repository.object_version(&aborted.id)?.state != UploadState::Aborted {
        return Err(RepositoryError::Unavailable);
    }
    if repository.object_version("missing") != Err(RepositoryError::NotFound) {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_equal<T: Eq>(actual: &T, expected: &T) -> Result<(), RepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

#[cfg(test)]
#[path = "repository_tests.rs"]
mod tests;
