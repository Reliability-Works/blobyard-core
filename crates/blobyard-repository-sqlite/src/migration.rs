use super::{SqliteRepository, map_error, rows, transfer_validation};
use blobyard_contract::{
    MigrationObjectRecord, MigrationRepository, MigrationRetentionRecord, MigrationShareRecord,
    MigrationSnapshot, ObjectChecksum, RepositoryError, ShareStatus, StorageKey,
};
use rusqlite::{Transaction, params};
use sha2::{Digest, Sha256};

impl MigrationRepository for SqliteRepository {
    fn import_migration(&self, snapshot: &MigrationSnapshot) -> Result<(), RepositoryError> {
        validate_snapshot(snapshot)?;
        self.write_transaction(|transaction| {
            require_empty(transaction)?;
            insert_workspaces(transaction, snapshot)?;
            insert_projects(transaction, snapshot)?;
            insert_objects(transaction, snapshot)?;
            insert_shares(transaction, snapshot)?;
            insert_retention(transaction, snapshot)
        })
    }
}

fn require_empty(transaction: &Transaction<'_>) -> Result<(), RepositoryError> {
    let occupied: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces UNION ALL SELECT 1 FROM projects UNION ALL SELECT 1 FROM object_versions UNION ALL SELECT 1 FROM shares UNION ALL SELECT 1 FROM retention_policies)",
            [],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if occupied {
        Err(RepositoryError::Conflict)
    } else {
        Ok(())
    }
}

fn insert_workspaces(
    transaction: &Transaction<'_>,
    snapshot: &MigrationSnapshot,
) -> Result<(), RepositoryError> {
    for workspace in &snapshot.workspaces {
        transaction
            .execute(
                "INSERT INTO workspaces (id, name, slug) VALUES (?1, ?2, ?3)",
                params![workspace.id, workspace.name, workspace.slug.as_str()],
            )
            .map_err(map_error)?;
    }
    Ok(())
}

fn insert_projects(
    transaction: &Transaction<'_>,
    snapshot: &MigrationSnapshot,
) -> Result<(), RepositoryError> {
    for project in &snapshot.projects {
        transaction
            .execute(
                "INSERT INTO projects (id, workspace_id, name, slug) VALUES (?1, ?2, ?3, ?4)",
                params![
                    project.id,
                    project.workspace_id,
                    project.name,
                    project.slug.as_str()
                ],
            )
            .map_err(map_error)?;
    }
    Ok(())
}

fn insert_objects(
    transaction: &Transaction<'_>,
    snapshot: &MigrationSnapshot,
) -> Result<(), RepositoryError> {
    for object in &snapshot.objects {
        insert_object(transaction, object)?;
    }
    Ok(())
}

fn insert_object(
    transaction: &Transaction<'_>,
    object: &MigrationObjectRecord,
) -> Result<(), RepositoryError> {
    let version = i64::try_from(object.version).map_err(|_error| RepositoryError::InvalidInput)?;
    let size = i64::try_from(object.size).map_err(|_error| RepositoryError::InvalidInput)?;
    let created =
        i64::try_from(object.created_at_ms).map_err(|_error| RepositoryError::InvalidInput)?;
    transaction
        .execute(
            "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms, source, git_repository, git_commit, git_branch) VALUES (?1, ?2, ?3, ?4, ?5, 'complete', ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                object.id,
                object.project_id,
                object.object_path,
                version,
                object.storage_key,
                size,
                object.checksum,
                created,
                object.source.as_str(),
                object.git_repository,
                object.git_commit,
                object.git_branch,
            ],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state, received_size, received_checksum, strategy) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 0, 'complete', ?4, ?5, 'single')",
            params![
                object.id,
                object.filename,
                object.content_type,
                size,
                object.checksum,
                retired_capability(&object.id),
            ],
        )
        .map(|_count| ())
        .map_err(map_error)
}

fn insert_shares(
    transaction: &Transaction<'_>,
    snapshot: &MigrationSnapshot,
) -> Result<(), RepositoryError> {
    for share in &snapshot.shares {
        let created = sql_time(share.created_at_ms)?;
        let expires = sql_time(share.expires_at_ms)?;
        let consumed = sql_time(share.consumed_count)?;
        let maximum = share.maximum_downloads.map(sql_time).transpose()?;
        let revoked = share.revoked_at_ms.map(sql_time).transpose()?;
        transaction
            .execute(
                "INSERT INTO shares (id, workspace_id, version_id, capability_hash, expires_at_ms, status, consumed_count, maximum_downloads, created_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    share.id,
                    share.workspace_id,
                    share.version_id,
                    share.capability_hash,
                    expires,
                    share.status.as_str(),
                    consumed,
                    maximum,
                    created,
                    revoked,
                ],
            )
            .map_err(map_error)?;
    }
    Ok(())
}

fn insert_retention(
    transaction: &Transaction<'_>,
    snapshot: &MigrationSnapshot,
) -> Result<(), RepositoryError> {
    for policy in &snapshot.retention {
        transaction
            .execute(
                "INSERT INTO retention_policies (project_id, keep_latest, path_glob, branch_glob, enabled, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    policy.project_id,
                    i64::from(policy.keep_latest),
                    policy.path_glob,
                    policy.branch_glob,
                    policy.enabled,
                    sql_time(policy.created_at_ms)?,
                    sql_time(policy.updated_at_ms)?,
                ],
            )
            .map_err(map_error)?;
    }
    Ok(())
}

fn validate_snapshot(snapshot: &MigrationSnapshot) -> Result<(), RepositoryError> {
    if snapshot.workspaces.is_empty()
        || !snapshot
            .workspaces
            .iter()
            .any(|workspace| workspace.id == "workspace_default")
    {
        return Err(RepositoryError::InvalidInput);
    }
    for workspace in &snapshot.workspaces {
        validate_texts([&workspace.id, &workspace.name])?;
    }
    for project in &snapshot.projects {
        validate_texts([&project.id, &project.workspace_id, &project.name])?;
    }
    for object in &snapshot.objects {
        validate_object(object)?;
    }
    for share in &snapshot.shares {
        validate_share(share)?;
    }
    for policy in &snapshot.retention {
        validate_retention(policy)?;
    }
    Ok(())
}

fn validate_object(object: &MigrationObjectRecord) -> Result<(), RepositoryError> {
    validate_texts([
        &object.id,
        &object.project_id,
        &object.object_path,
        &object.filename,
        &object.content_type,
    ])?;
    if object.version == 0 {
        return Err(RepositoryError::InvalidInput);
    }
    StorageKey::new(object.storage_key.clone()).map_err(|_error| RepositoryError::InvalidInput)?;
    ObjectChecksum::new(object.checksum.clone()).map_err(|_error| RepositoryError::InvalidInput)?;
    transfer_validation::validate_provenance(
        object.git_repository.as_deref(),
        object.git_commit.as_deref(),
        object.git_branch.as_deref(),
    )
}

fn validate_share(share: &MigrationShareRecord) -> Result<(), RepositoryError> {
    validate_texts([&share.id, &share.workspace_id, &share.version_id])?;
    ObjectChecksum::new(share.capability_hash.clone())
        .map_err(|_error| RepositoryError::InvalidInput)?;
    let counts_valid = share.maximum_downloads.is_none_or(|maximum| {
        maximum > 0
            && share.consumed_count <= maximum
            && (share.status != ShareStatus::Exhausted || share.consumed_count == maximum)
    });
    let status_valid = match share.status {
        ShareStatus::Revoked => share.revoked_at_ms.is_some(),
        ShareStatus::Active | ShareStatus::Exhausted => share.revoked_at_ms.is_none(),
    };
    let revoked_valid = share
        .revoked_at_ms
        .is_none_or(|revoked| revoked >= share.created_at_ms);
    if share.expires_at_ms > share.created_at_ms && counts_valid && status_valid && revoked_valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn validate_retention(policy: &MigrationRetentionRecord) -> Result<(), RepositoryError> {
    validate_texts([&policy.project_id])?;
    if policy.keep_latest == 0 || policy.updated_at_ms < policy.created_at_ms {
        Err(RepositoryError::InvalidInput)
    } else {
        for value in [&policy.path_glob, &policy.branch_glob]
            .into_iter()
            .flatten()
        {
            rows::validate_text(value)?;
        }
        Ok(())
    }
}

fn validate_texts<const N: usize>(values: [&str; N]) -> Result<(), RepositoryError> {
    for value in values {
        rows::validate_text(value)?;
    }
    Ok(())
}

fn sql_time(value: u64) -> Result<i64, RepositoryError> {
    i64::try_from(value).map_err(|_error| RepositoryError::InvalidInput)
}

fn retired_capability(id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"blobyard-migration-reservation\0");
    digest.update(id.as_bytes());
    blobyard_core::hex_digest(&digest.finalize())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "migration_failure_tests.rs"]
mod failure_tests;
