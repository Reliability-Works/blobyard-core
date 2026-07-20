use blobyard_contract::{
    CiAction, LocalApiTokenRecord, LocalCiTrustRecord, LocalCliSessionRecord,
    LocalMachineSessionRecord, ObjectSource, ObjectVersionRecord, PreviewRecord, PreviewStatus,
    ProjectRecord, RepositoryError, ReservationState, ReservationStrategy, ShareRecord,
    ShareStatus, StoredObjectRecord, UploadPartRecord, UploadReservationRecord, UploadState,
    WorkspaceRecord,
};
use blobyard_core::Slug;
use rusqlite::Row;

pub(super) const OBJECT_VERSION_COLUMNS: &str = "id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms, source, git_repository, git_commit, git_branch";
pub(super) const STORED_COLUMNS: &str = "v.id, v.project_id, v.object_path, v.version, v.storage_key, v.state, v.size, v.checksum, v.created_at_ms, v.source, v.git_repository, v.git_commit, v.git_branch, r.filename, r.content_type";
pub(super) const API_TOKEN_COLUMNS: &str = "id, name, secret_hash, scopes, workspace_id, token_prefix, project_id, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms";
pub(super) const CLI_SESSION_COLUMNS: &str = "id, token_id, workspace_id, name, platform, version, created_at_ms, last_used_at_ms, revoked_at_ms";
pub(super) const CI_TRUST_COLUMNS: &str = "id, workspace_id, project_id, repository, workflow_path, workflow_ref, allowed_ref_glob, environment, allowed_actions, audience, created_at_ms, revoked_at_ms";
pub(super) const MACHINE_SESSION_COLUMNS: &str = "id, trust_id, workspace_id, project_id, repository, git_ref, run_id, run_attempt, actions, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms";
pub(super) const SHARE_COLUMNS: &str = "id, workspace_id, version_id, expires_at_ms, status, consumed_count, maximum_downloads, created_at_ms, revoked_at_ms";
pub(super) const PREVIEW_COLUMNS: &str =
    "id, workspace_id, project_id, expires_at_ms, status, created_at_ms, revoked_at_ms";

pub(super) fn workspace(row: &Row<'_>) -> rusqlite::Result<WorkspaceRecord> {
    Ok(WorkspaceRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        slug: slug(row.get(2)?)?,
    })
}

pub(super) fn project(row: &Row<'_>) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        slug: slug(row.get(3)?)?,
    })
}

pub(super) fn object_version(row: &Row<'_>) -> rusqlite::Result<ObjectVersionRecord> {
    let version: i64 = row.get(3)?;
    let state: String = row.get(5)?;
    Ok(ObjectVersionRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        object_path: row.get(2)?,
        version: u64::try_from(version).map_err(conversion_error)?,
        storage_key: row.get(4)?,
        state: UploadState::parse(&state).ok_or_else(|| conversion_error(state))?,
        size: optional_u64(row.get(6)?)?,
        checksum: row.get(7)?,
        created_at_ms: required_u64(row.get(8)?)?,
        source: object_source(row.get(9)?)?,
        git_repository: row.get(10)?,
        git_commit: row.get(11)?,
        git_branch: row.get(12)?,
    })
}

pub(super) fn api_token(row: &Row<'_>) -> rusqlite::Result<LocalApiTokenRecord> {
    let scopes: String = row.get(3)?;
    Ok(LocalApiTokenRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        secret_hash: row.get(2)?,
        scopes: scopes.split('\n').map(str::to_owned).collect(),
        workspace_id: row.get(4)?,
        token_prefix: row.get(5)?,
        project_id: row.get(6)?,
        created_at_ms: required_u64(row.get(7)?)?,
        expires_at_ms: required_u64(row.get(8)?)?,
        last_used_at_ms: optional_u64(row.get(9)?)?,
        revoked_at_ms: optional_u64(row.get(10)?)?,
    })
}

pub(super) fn cli_session(row: &Row<'_>) -> rusqlite::Result<LocalCliSessionRecord> {
    Ok(LocalCliSessionRecord {
        id: row.get(0)?,
        token_id: row.get(1)?,
        workspace_id: row.get(2)?,
        name: row.get(3)?,
        platform: row.get(4)?,
        version: row.get(5)?,
        created_at_ms: required_u64(row.get(6)?)?,
        last_used_at_ms: optional_u64(row.get(7)?)?,
        revoked_at_ms: optional_u64(row.get(8)?)?,
    })
}

pub(super) fn ci_trust(row: &Row<'_>) -> rusqlite::Result<LocalCiTrustRecord> {
    let actions: String = row.get(8)?;
    Ok(LocalCiTrustRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_id: row.get(2)?,
        repository: row.get(3)?,
        workflow_path: row.get(4)?,
        workflow_ref: row.get(5)?,
        allowed_ref_glob: row.get(6)?,
        environment: row.get(7)?,
        allowed_actions: ci_actions(&actions)?,
        audience: row.get(9)?,
        created_at_ms: required_u64(row.get(10)?)?,
        revoked_at_ms: optional_u64(row.get(11)?)?,
    })
}

pub(super) fn machine_session(row: &Row<'_>) -> rusqlite::Result<LocalMachineSessionRecord> {
    let actions: String = row.get(8)?;
    Ok(LocalMachineSessionRecord {
        id: row.get(0)?,
        trust_id: row.get(1)?,
        workspace_id: row.get(2)?,
        project_id: row.get(3)?,
        repository: row.get(4)?,
        git_ref: row.get(5)?,
        run_id: row.get(6)?,
        run_attempt: row.get(7)?,
        actions: ci_actions(&actions)?,
        created_at_ms: required_u64(row.get(9)?)?,
        expires_at_ms: required_u64(row.get(10)?)?,
        last_used_at_ms: optional_u64(row.get(11)?)?,
        revoked_at_ms: optional_u64(row.get(12)?)?,
    })
}

pub(super) fn upload_reservation(row: &Row<'_>) -> rusqlite::Result<UploadReservationRecord> {
    let state: String = row.get(19)?;
    let strategy: String = row.get(20)?;
    Ok(UploadReservationRecord {
        id: row.get(0)?,
        version: object_version_from(row, 1)?,
        filename: row.get(14)?,
        content_type: row.get(15)?,
        expected_size: required_u64(row.get(16)?)?,
        expected_checksum: row.get(17)?,
        expires_at_ms: required_u64(row.get(18)?)?,
        state: ReservationState::parse(&state).ok_or_else(|| conversion_error(state))?,
        strategy: ReservationStrategy::parse(&strategy)
            .ok_or_else(|| conversion_error(strategy))?,
        part_size: optional_u64(row.get(21)?)?,
        part_count: optional_u32(row.get(22)?)?,
        provider_upload_id: row.get(23)?,
    })
}

pub(super) fn upload_part(row: &Row<'_>) -> rusqlite::Result<UploadPartRecord> {
    Ok(UploadPartRecord {
        upload_id: row.get(0)?,
        part_number: required_u32(row.get(1)?)?,
        expected_size: required_u64(row.get(2)?)?,
        expires_at_ms: required_u64(row.get(3)?)?,
        received_size: optional_u64(row.get(4)?)?,
        received_checksum: row.get(5)?,
        provider_tag: row.get(6)?,
    })
}

pub(super) fn stored_object(row: &Row<'_>) -> rusqlite::Result<StoredObjectRecord> {
    stored_object_from(row, 0)
}

pub(super) fn stored_object_from(
    row: &Row<'_>,
    offset: usize,
) -> rusqlite::Result<StoredObjectRecord> {
    Ok(StoredObjectRecord {
        version: object_version_from(row, offset)?,
        filename: row.get(offset + 13)?,
        content_type: row.get(offset + 14)?,
    })
}

pub(super) fn share(row: &Row<'_>) -> rusqlite::Result<ShareRecord> {
    let status: String = row.get(4)?;
    Ok(ShareRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        version_id: row.get(2)?,
        expires_at_ms: required_u64(row.get(3)?)?,
        status: ShareStatus::parse(&status).ok_or_else(|| conversion_error(status))?,
        consumed_count: required_u64(row.get(5)?)?,
        maximum_downloads: optional_u64(row.get(6)?)?,
        created_at_ms: required_u64(row.get(7)?)?,
        revoked_at_ms: optional_u64(row.get(8)?)?,
    })
}

pub(super) fn preview(row: &Row<'_>) -> rusqlite::Result<PreviewRecord> {
    let status: String = row.get(4)?;
    Ok(PreviewRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_id: row.get(2)?,
        expires_at_ms: required_u64(row.get(3)?)?,
        status: PreviewStatus::parse(&status).ok_or_else(|| conversion_error(status))?,
        created_at_ms: required_u64(row.get(5)?)?,
        revoked_at_ms: optional_u64(row.get(6)?)?,
    })
}

pub(super) fn validate_text(value: &str) -> Result<(), RepositoryError> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(RepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn slug(value: String) -> rusqlite::Result<Slug> {
    Slug::new(value.clone()).map_err(|_error| conversion_error(value))
}

fn object_source(value: String) -> rusqlite::Result<ObjectSource> {
    ObjectSource::parse(&value).ok_or_else(|| conversion_error(value))
}

fn ci_actions(value: &str) -> rusqlite::Result<Vec<CiAction>> {
    value
        .split('\n')
        .map(|action| CiAction::parse(action).ok_or_else(|| conversion_error(action.to_owned())))
        .collect()
}

fn optional_u64(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value
        .map(|number| u64::try_from(number).map_err(conversion_error))
        .transpose()
}

fn required_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(conversion_error)
}

fn optional_u32(value: Option<i64>) -> rusqlite::Result<Option<u32>> {
    value
        .map(|number| u32::try_from(number).map_err(conversion_error))
        .transpose()
}

fn required_u32(value: i64) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(conversion_error)
}

fn object_version_from(row: &Row<'_>, offset: usize) -> rusqlite::Result<ObjectVersionRecord> {
    let version: i64 = row.get(offset + 3)?;
    let state: String = row.get(offset + 5)?;
    Ok(ObjectVersionRecord {
        id: row.get(offset)?,
        project_id: row.get(offset + 1)?,
        object_path: row.get(offset + 2)?,
        version: required_u64(version)?,
        storage_key: row.get(offset + 4)?,
        state: UploadState::parse(&state).ok_or_else(|| conversion_error(state))?,
        size: optional_u64(row.get(offset + 6)?)?,
        checksum: row.get(offset + 7)?,
        created_at_ms: required_u64(row.get(offset + 8)?)?,
        source: object_source(row.get(offset + 9)?)?,
        git_repository: row.get(offset + 10)?,
        git_commit: row.get(offset + 11)?,
        git_branch: row.get(offset + 12)?,
    })
}

pub(super) fn conversion_error(
    value: impl std::fmt::Debug + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(ConversionFailure(format!("{value:?}"))),
    )
}

#[derive(Debug)]
struct ConversionFailure(String);

impl std::fmt::Display for ConversionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ConversionFailure {}

#[cfg(test)]
#[path = "rows_tests.rs"]
pub(super) mod tests;
