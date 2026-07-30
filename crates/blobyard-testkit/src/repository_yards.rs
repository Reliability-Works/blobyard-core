use blobyard_contract::{
    LifecycleRepository, LocalUserRepository, MetadataRepository, NewAuditEvent, RepositoryError,
    TransferRepository, WebYardRepository, WebYardStatus, WorkspaceGroupRepository,
    YardDeployStatus, YardGuestRepository, YardIdentityRepository, YardOidcRepository,
    YardSessionRepository,
};
use blobyard_core::{Slug, SlugError};

#[path = "repository_yards_access.rs"]
mod access;
#[path = "repository_yards_delivery.rs"]
mod delivery;
#[cfg(test)]
#[path = "repository_yards_fixture_tests.rs"]
mod fixture_tests;
#[path = "repository_yards_fixtures.rs"]
mod fixtures;
#[path = "repository_yards_guests.rs"]
mod guests;
#[path = "repository_yards_oidc.rs"]
mod oidc;
#[path = "repository_yards_policy.rs"]
mod policy;
#[path = "repository_yards_session_direct.rs"]
mod session_direct;
#[path = "repository_yards_session_fixtures.rs"]
mod session_fixtures;
#[path = "repository_yards_session_grants.rs"]
mod session_grants;
#[path = "repository_yards_session_groups.rs"]
mod session_groups;
#[path = "repository_yards_session_revocation.rs"]
mod session_revocation;
#[path = "repository_yards_sessions.rs"]
mod sessions;
#[path = "repository_yards_lifecycle.rs"]
mod yard_lifecycle;

use crate::FixtureExecutionTracker;
use fixtures::{action_event, event, new_deploy, new_yard};
pub use fixtures::{
    granted_event, new_grant, revoked_event, visibility_event, yard_application_policy,
    yard_owner_event, yard_policy_event,
};
use yard_lifecycle::{finalise, finalise_as, start};

/// Combined repository surface needed by Web Yard conformance.
pub trait YardConformanceRepository:
    WebYardRepository
    + MetadataRepository
    + TransferRepository
    + LocalUserRepository
    + WorkspaceGroupRepository
    + YardSessionRepository
    + YardOidcRepository
    + YardGuestRepository
    + YardIdentityRepository
    + LifecycleRepository
{
}

impl<
    T: WebYardRepository
        + MetadataRepository
        + TransferRepository
        + LocalUserRepository
        + WorkspaceGroupRepository
        + YardSessionRepository
        + YardOidcRepository
        + YardGuestRepository
        + YardIdentityRepository
        + LifecycleRepository,
> YardConformanceRepository for T
{
}

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
    yard_conformance_with_guest_capacity(repository, fixture, true)
}

/// Runs the Web Yard operation surface used by exhaustive repository fault injection.
///
/// The exact 100-invitation capacity case remains part of [`yard_conformance`].
/// This variant omits only that repeated create sequence so fault sweeps cover
/// each distinct operation boundary without replaying an equivalent statement
/// one hundred times.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn yard_fault_conformance(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
) -> Result<(), RepositoryError> {
    yard_conformance_with_guest_capacity(repository, fixture, false)
}

fn yard_conformance_with_guest_capacity(
    repository: &dyn YardConformanceRepository,
    fixture: &YardConformanceFixture,
    include_guest_capacity: bool,
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
    session_fixtures::assert_production_environment(repository, &first.yard.id)?;
    if !repository
        .list_yard_environments("yard_unknown")?
        .is_empty()
    {
        return Err(RepositoryError::Unavailable);
    }
    sessions::create_session_user(repository)?;
    policy::approve_application_policy(repository, &first.yard.id)?;
    access::assert_access_controls(repository, &first, &version_id)?;
    oidc::assert_member_and_attempt_controls(repository, &first)?;
    let mut tracker = FixtureExecutionTracker::new("testkit", "yard-sessions");
    sessions::assert_session_controls(repository, &first, &version_id, &mut tracker)?;
    guests::assert_guest_controls(repository, &first, include_guest_capacity)?;
    assert_replacement_and_rollback(repository, fixture, &first, &version_id)?;
    assert_failure_and_history(repository, fixture, &version_id)?;
    assert_yard_deletion(repository, &first)?;
    tracker.finish()
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
    delivery::assert_delivery(repository, &first_live.yard.host_label, version_id)?;
    delivery::assert_delivery(
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
    delivery::prune_history(repository, &fixture.history_name, version_id)?;
    delivery::assert_deleted_yard_cannot_finalise(repository, &fixture.inactive_name, version_id)
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
        || repository.yard_file_by_host(&first.yard.host_label, "", None, 100)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}
