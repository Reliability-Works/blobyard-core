#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::ServerError;
use blobyard_contract::DeletionPlan;
use blobyard_core::{GeneratedSecretKind, SecretString};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;

/// Applies an already-durable retention plan through the production core.
///
/// # Errors
///
/// Returns the stable repository or storage failure produced by the plan.
pub fn enforce_plan(
    repository: &SqliteRepository,
    storage: &FilesystemStorage,
    workspace_id: String,
    started_at_ms: u64,
    plan: DeletionPlan,
) -> Result<(), ServerError> {
    super::enforce_plan_with_clock(
        repository,
        storage,
        workspace_id,
        started_at_ms,
        plan,
        super::current_time,
    )
}

/// Exercises a clock failure before a retention run can begin.
///
/// # Errors
///
/// Always returns the stable initialization failure after namespace resolution.
pub fn enforce_project_clock_failure(
    repository: &SqliteRepository,
    storage: &FilesystemStorage,
    project_id: &str,
) -> Result<(), ServerError> {
    super::enforce_project_with_clock(repository, storage, project_id, failed_clock)
}

/// Exercises a clock failure after retained storage objects have been deleted.
///
/// # Errors
///
/// Returns the stable initialization failure after the plan reaches finalization.
pub fn enforce_plan_clock_failure(
    repository: &SqliteRepository,
    storage: &FilesystemStorage,
    workspace_id: String,
    started_at_ms: u64,
    plan: DeletionPlan,
) -> Result<(), ServerError> {
    super::enforce_plan_with_clock(
        repository,
        storage,
        workspace_id,
        started_at_ms,
        plan,
        failed_clock,
    )
}

/// Exercises a runtime-secret write failure after creating its secure temporary file.
///
/// # Errors
///
/// Returns the stable data-directory failure without persisting a runtime secret.
pub fn runtime_secret_write_failure(
    data_directory: &std::path::Path,
    temporary: tempfile::NamedTempFile,
) -> Result<(), ServerError> {
    let path = data_directory.join("runtime.secret");
    let secret = SecretString::from_generated_entropy(GeneratedSecretKind::RuntimeSecret, [0; 32]);
    crate::persist_runtime_secret(
        data_directory,
        &path,
        temporary,
        secret,
        Err(std::io::Error::other("fixture write failure")),
    )
    .map(|_secret| ())
}

/// Resolves the parent workspace through the production repository lookup.
///
/// # Errors
///
/// Returns the stable repository failure when the namespace cannot be resolved.
pub fn project_workspace(
    repository: &SqliteRepository,
    project_id: &str,
) -> Result<String, ServerError> {
    super::project_workspace(repository, project_id)
}

/// Exercises rejection of an invalid built-in workspace slug.
///
/// # Errors
///
/// Always returns the stable initialization failure.
pub fn invalid_default_slug(repository: &SqliteRepository) -> Result<(), ServerError> {
    super::default_workspace_with_slug(repository, Err(blobyard_core::SlugError)).map(|_value| ())
}

/// Creates or reads the default workspace through the production helper.
///
/// # Errors
///
/// Returns the stable repository failure when the workspace cannot be read or created.
pub fn default_workspace(repository: &SqliteRepository) -> Result<(), ServerError> {
    super::default_workspace(repository).map(|_value| ())
}

const fn failed_clock() -> Result<u64, ServerError> {
    Err(ServerError::Initialization)
}
