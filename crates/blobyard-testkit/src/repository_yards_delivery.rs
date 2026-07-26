use super::fixtures::event;
use super::{YardConformanceRepository, finalise, start};
use blobyard_contract::{RepositoryError, YardDeployStatus};
use blobyard_core::Slug;

pub(super) fn assert_delivery(
    repository: &dyn YardConformanceRepository,
    host: &str,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let index = repository.yard_file_by_host(host, "", None, 0)?;
    let exact = repository.yard_file_by_host(host, "asset.js", None, 0)?;
    let directory = repository.yard_file_by_host(host, "docs/", None, 0)?;
    let clean = repository.yard_file_by_host(host, "guide", None, 0)?;
    let spa = repository.yard_file_by_host(host, "missing", None, 0)?;
    let missing = repository.yard_file_by_host(host, "missing.txt", None, 0)?;
    if index.object.version.id == version_id
        && !index.not_found_document
        && exact.object.version.id == version_id
        && !exact.not_found_document
        && directory.object.version.id == version_id
        && !directory.not_found_document
        && clean.object.version.id == version_id
        && !clean.not_found_document
        && spa.object.version.id == version_id
        && !spa.not_found_document
        && missing.object.version.id == version_id
        && missing.not_found_document
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

pub(super) fn assert_deleted_yard_cannot_finalise(
    repository: &dyn YardConformanceRepository,
    name: &Slug,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let started = start(repository, name, 50)?;
    repository.delete_web_yard(
        &started.yard.id,
        51,
        &event("yard.deleted", "web_yard", "yardId", &started.yard.id, 51),
    )?;
    if finalise(repository, &started.deploy.id, version_id, 5, 52) != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

pub(super) fn prune_history(
    repository: &dyn YardConformanceRepository,
    name: &Slug,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let oldest = start(repository, name, 10)?.deploy;
    finalise(repository, &oldest.id, version_id, 5, 110)?;
    for number in 11..=20 {
        let started = start(repository, name, number)?;
        finalise(repository, &started.deploy.id, version_id, 5, number + 100)?;
    }
    if repository.yard_deploy_by_id(&oldest.id)?.status != YardDeployStatus::Pruned
        || repository.yard_file_by_host(&oldest.deployment_host_label, "", None, 0)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}
