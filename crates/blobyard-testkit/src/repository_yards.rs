use blobyard_contract::{
    NewAuditEvent, NewYardFile, RepositoryError, TransferRepository, WebYardRepository,
    WebYardStatus, YardDeployStatus,
};
use blobyard_core::{Slug, SlugError};

#[cfg(test)]
#[path = "repository_yards_fixture_tests.rs"]
mod fixture_tests;
#[path = "repository_yards_fixtures.rs"]
mod fixtures;
use fixtures::{action_event, deployed_event, event, new_deploy, new_yard};

/// Combined repository surface needed by Web Yard conformance.
pub trait YardConformanceRepository: WebYardRepository + TransferRepository {}

impl<T: WebYardRepository + TransferRepository> YardConformanceRepository for T {}

/// Validated names used to exercise distinct Web Yard lifecycles.
pub struct YardConformanceFixture {
    /// Name used for deployment, replacement, rollback, failure, and deletion.
    pub primary_name: Slug,
    /// Name used to prove finalisation is rejected after Yard deletion.
    pub inactive_name: Slug,
    /// Name used to prove bounded deployment history pruning.
    pub history_name: Slug,
}

impl YardConformanceFixture {
    /// Validates the distinct Yard names used by portable conformance.
    ///
    /// # Errors
    ///
    /// Returns the first invalid Yard name.
    pub fn new(
        primary_name: &str,
        inactive_name: &str,
        history_name: &str,
    ) -> Result<Self, SlugError> {
        Ok(Self {
            primary_name: Slug::new(primary_name)?,
            inactive_name: Slug::new(inactive_name)?,
            history_name: Slug::new(history_name)?,
        })
    }
}

/// Builds a deterministic single-target Web Yard audit fixture.
#[must_use]
pub fn yard_event(
    action: &str,
    target_type: &str,
    target_key: &str,
    target_id: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    super::events::capability_event(action, target_type, target_key, target_id, created_at_ms)
}

/// Runs deterministic start, finalise, delivery, failure, rollback, pruning, and deletion checks.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn yard_conformance(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
) -> Result<(), RepositoryError> {
    if !repository.list_web_yards("project_fixture")?.is_empty() {
        return Err(RepositoryError::Unavailable);
    }
    let version_id = repository
        .list_stored_objects("project_fixture", Some("artifacts/build.zip"), false)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?
        .version
        .id;
    let first = assert_initial_deployment(repository, fixture, &version_id)?;
    assert_replacement_and_rollback(repository, fixture, &first, &version_id)?;
    assert_failure_and_history(repository, fixture, &version_id)?;
    assert_yard_deletion(repository, &first)
}

fn assert_initial_deployment(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
    version_id: &str,
) -> Result<blobyard_contract::YardStartRecord, RepositoryError> {
    let first = start(repository, &fixture.primary_name, 1)?;
    let reused = repository.start_yard_deploy(
        &new_yard(&fixture.primary_name, 99),
        &new_deploy(&fixture.primary_name, 1, "yard_docs_99"),
        &event("yard.created", "web_yard", "yardId", "yard_docs_99", 99),
    )?;
    if reused != first {
        return Err(RepositoryError::Unavailable);
    }
    let first_live = finalise(repository, &first.deploy.id, version_id, 5, 10)?;
    assert_delivery(repository, &first_live.yard.host_label, version_id)?;
    assert_delivery(
        repository,
        &first_live.deploy.deployment_host_label,
        version_id,
    )?;
    Ok(first)
}

fn assert_replacement_and_rollback(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
    first: &blobyard_contract::YardStartRecord,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let second = start(repository, &fixture.primary_name, 2)?;
    let second_live = finalise(repository, &second.deploy.id, version_id, 5, 20)?;
    if second_live.deploy.status != YardDeployStatus::Live
        || repository.yard_deploy_by_id(&first.deploy.id)?.status != YardDeployStatus::Superseded
    {
        return Err(RepositoryError::Unavailable);
    }
    let delayed = start(repository, &fixture.primary_name, 4)?;
    let newest = start(repository, &fixture.primary_name, 5)?;
    let newest_live = finalise(repository, &newest.deploy.id, version_id, 5, 25)?;
    let delayed_terminal = finalise_as(
        repository,
        &delayed.deploy.id,
        version_id,
        5,
        26,
        "superseded",
    )?;
    if newest_live.deploy.status != YardDeployStatus::Live
        || delayed_terminal.deploy.status != YardDeployStatus::Superseded
        || delayed_terminal.yard.current_deploy_id.as_deref() != Some(newest.deploy.id.as_str())
    {
        return Err(RepositoryError::Unavailable);
    }
    let rolled_back = repository.rollback_web_yard(
        &first.yard.id,
        Some(&first.deploy.id),
        30,
        &action_event("yard.rolled_back", &first.yard.id, &first.deploy.id, 30),
    )?;
    if rolled_back.deploy.status != YardDeployStatus::Live
        || rolled_back.yard.current_deploy_id.as_deref() != Some(first.deploy.id.as_str())
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_failure_and_history(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let failed = start(repository, &fixture.primary_name, 3)?;
    let failed_record = repository.fail_yard_deploy(
        &failed.deploy.id,
        "UPLOAD_FAILED",
        "The fixture upload failed.",
        40,
    )?;
    if failed_record.status != YardDeployStatus::Failed
        || repository.fail_yard_deploy(&failed.deploy.id, "IGNORED", "An idempotent retry.", 41)?
            != failed_record
    {
        return Err(RepositoryError::Unavailable);
    }
    prune_history(repository, &fixture.history_name, version_id)?;
    assert_deleted_yard_cannot_finalise(repository, &fixture.inactive_name, version_id)
}

fn assert_yard_deletion(
    repository: &dyn YardConformanceRepository,
    first: &blobyard_contract::YardStartRecord,
) -> Result<(), RepositoryError> {
    let yards = repository.list_web_yards("project_fixture")?;
    if yards.len() != 2 || yards[1].id != first.yard.id {
        return Err(RepositoryError::Unavailable);
    }
    let deleted = repository.delete_web_yard(
        &first.yard.id,
        100,
        &event("yard.deleted", "web_yard", "yardId", &first.yard.id, 100),
    )?;
    if !deleted
        || repository.delete_web_yard(
            &first.yard.id,
            101,
            &event("yard.deleted", "web_yard", "yardId", &first.yard.id, 101),
        )?
        || repository.web_yard_by_id(&first.yard.id)?.status != WebYardStatus::Deleted
        || repository.yard_file_by_host(&first.yard.host_label, "")
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn start(
    repository: &dyn YardConformanceRepository,
    name: &Slug,
    number: u64,
) -> Result<blobyard_contract::YardStartRecord, RepositoryError> {
    let yard = new_yard(name, number);
    repository.start_yard_deploy(
        &yard,
        &new_deploy(name, number, &yard.id),
        &event("yard.created", "web_yard", "yardId", &yard.id, number),
    )
}

fn finalise(
    repository: &dyn YardConformanceRepository,
    deploy_id: &str,
    version_id: &str,
    byte_size: u64,
    at: u64,
) -> Result<blobyard_contract::YardDeploymentRecord, RepositoryError> {
    finalise_as(repository, deploy_id, version_id, byte_size, at, "live")
}

fn finalise_as(
    repository: &dyn YardConformanceRepository,
    deploy_id: &str,
    version_id: &str,
    byte_size: u64,
    at: u64,
    status: &str,
) -> Result<blobyard_contract::YardDeploymentRecord, RepositoryError> {
    repository.finalise_yard_deploy(
        deploy_id,
        &[
            NewYardFile {
                normalized_path: "404.html".to_owned(),
                version_id: version_id.to_owned(),
                byte_size,
            },
            NewYardFile {
                normalized_path: "asset.js".to_owned(),
                version_id: version_id.to_owned(),
                byte_size,
            },
            NewYardFile {
                normalized_path: "docs/index.html".to_owned(),
                version_id: version_id.to_owned(),
                byte_size,
            },
            NewYardFile {
                normalized_path: "guide.html".to_owned(),
                version_id: version_id.to_owned(),
                byte_size,
            },
            NewYardFile {
                normalized_path: "index.html".to_owned(),
                version_id: version_id.to_owned(),
                byte_size,
            },
        ],
        at,
        &deployed_event(deploy_id, 5, byte_size * 5, status, at),
    )
}

fn assert_delivery(
    repository: &dyn YardConformanceRepository,
    host: &str,
    version_id: &str,
) -> Result<(), RepositoryError> {
    let index = repository.yard_file_by_host(host, "")?;
    let exact = repository.yard_file_by_host(host, "asset.js")?;
    let directory = repository.yard_file_by_host(host, "docs/")?;
    let clean = repository.yard_file_by_host(host, "guide")?;
    let spa = repository.yard_file_by_host(host, "missing")?;
    let missing = repository.yard_file_by_host(host, "missing.txt")?;
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

fn assert_deleted_yard_cannot_finalise(
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

fn prune_history(
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
        || repository.yard_file_by_host(&oldest.deployment_host_label, "")
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}
