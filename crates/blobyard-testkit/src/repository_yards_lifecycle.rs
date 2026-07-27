use super::{
    YardConformanceRepository,
    fixtures::{deployed_event, event, new_deploy, new_yard},
};
use blobyard_contract::{NewYardFile, RepositoryError, YardDeploymentRecord, YardStartRecord};
use blobyard_core::Slug;

pub(super) fn start(
    repository: &dyn YardConformanceRepository,
    name: &Slug,
    number: u64,
) -> Result<YardStartRecord, RepositoryError> {
    let yard = new_yard(name, number);
    repository.start_yard_deploy(
        &yard,
        &new_deploy(name, number, &yard.id),
        &event("yard.created", "web_yard", "yardId", &yard.id, number),
    )
}

pub(super) fn finalise(
    repository: &dyn YardConformanceRepository,
    deploy_id: &str,
    version_id: &str,
    byte_size: u64,
    at: u64,
) -> Result<YardDeploymentRecord, RepositoryError> {
    finalise_as(repository, deploy_id, version_id, byte_size, at, "live")
}

pub(super) fn finalise_as(
    repository: &dyn YardConformanceRepository,
    deploy_id: &str,
    version_id: &str,
    byte_size: u64,
    at: u64,
    status: &str,
) -> Result<YardDeploymentRecord, RepositoryError> {
    repository.finalise_yard_deploy(
        deploy_id,
        &[
            file("404.html", version_id, byte_size),
            file("asset.js", version_id, byte_size),
            file("docs/index.html", version_id, byte_size),
            file("guide.html", version_id, byte_size),
            file("index.html", version_id, byte_size),
        ],
        at,
        &deployed_event(deploy_id, 5, byte_size * 5, status, at),
    )
}

fn file(normalized_path: &str, version_id: &str, byte_size: u64) -> NewYardFile {
    NewYardFile {
        normalized_path: normalized_path.to_owned(),
        version_id: version_id.to_owned(),
        byte_size,
    }
}
