use super::{
    SqliteRepository, auth_validation, finish_audited_change, lifecycle_audit, map_error,
    preview_queries, rows, transfer_validation,
};
use blobyard_contract::{
    NewAuditEvent, NewPreview, NewPreviewFile, PreviewRecord, PreviewRepository, PreviewStatus,
    PreviewTarget, RepositoryError, is_valid_preview_path,
};
use rusqlite::{Transaction, params};
use std::collections::HashSet;

const MAXIMUM_PREVIEW_FILES: usize = 10_000;

impl PreviewRepository for SqliteRepository {
    fn create_preview(
        &self,
        preview: &NewPreview,
        event: &NewAuditEvent,
    ) -> Result<PreviewRecord, RepositoryError> {
        let times = validate_new_preview(preview)?;
        validate_event(event, "preview.created", preview, preview.created_at_ms)?;
        self.write_transaction(|transaction| {
            insert_preview(transaction, preview, &times)?;
            for file in &preview.files {
                insert_file(transaction, preview, file)?;
            }
            lifecycle_audit::insert(transaction, event)?;
            preview_queries::by_id(transaction, &preview.id)
        })
    }

    fn list_previews(&self, project_id: &str) -> Result<Vec<PreviewRecord>, RepositoryError> {
        rows::validate_text(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM previews WHERE project_id = ?1 ORDER BY created_at_ms DESC, id DESC",
                rows::PREVIEW_COLUMNS
            ))
            .map_err(map_error)?;
        let result = preview_queries::list(&mut statement, project_id);
        drop(statement);
        drop(connection);
        result
    }

    fn preview_by_id(&self, preview_id: &str) -> Result<PreviewRecord, RepositoryError> {
        rows::validate_text(preview_id)?;
        let connection = self.connection()?;
        preview_queries::by_id(&connection, preview_id)
    }

    fn preview_file_by_capability(
        &self,
        capability_hash: &str,
        normalized_path: &str,
        now_ms: u64,
    ) -> Result<PreviewTarget, RepositoryError> {
        auth_validation::validate_hash(capability_hash)?;
        validate_manifest_path(normalized_path)?;
        let connection = self.connection()?;
        preview_queries::target_by_capability(
            &connection,
            capability_hash,
            normalized_path,
            transfer_validation::to_i64(now_ms)?,
        )
    }

    fn revoke_preview(
        &self,
        preview_id: &str,
        workspace_id: &str,
        project_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        for value in [preview_id, workspace_id, project_id] {
            rows::validate_text(value)?;
        }
        self.write_transaction(|transaction| {
            let preview = preview_queries::by_id(transaction, preview_id)?;
            if preview.workspace_id != workspace_id || preview.project_id != project_id {
                return Err(RepositoryError::NotFound);
            }
            if preview.status == PreviewStatus::Revoked {
                return Ok(false);
            }
            validate_event(event, "preview.revoked", &preview, revoked_at_ms)?;
            let changed = transaction
                .execute(
                    "UPDATE previews SET status = 'revoked', revoked_at_ms = ?4 WHERE id = ?1 AND workspace_id = ?2 AND project_id = ?3 AND status = 'active'",
                    params![
                        preview_id,
                        workspace_id,
                        project_id,
                        transfer_validation::to_i64(revoked_at_ms)?,
                    ],
                )
                .map_err(map_error)?;
            finish_audited_change(transaction, changed, event)
        })
    }
}

struct PreviewTimes {
    expires: i64,
    created: i64,
}

fn validate_new_preview(preview: &NewPreview) -> Result<PreviewTimes, RepositoryError> {
    for value in [&preview.id, &preview.workspace_id, &preview.project_id] {
        rows::validate_text(value)?;
    }
    auth_validation::validate_hash(&preview.capability_hash)?;
    let times = PreviewTimes {
        expires: transfer_validation::to_i64(preview.expires_at_ms)?,
        created: transfer_validation::to_i64(preview.created_at_ms)?,
    };
    let files_valid = !preview.files.is_empty()
        && preview.files.len() <= MAXIMUM_PREVIEW_FILES
        && preview
            .files
            .iter()
            .any(|file| file.normalized_path == "index.html");
    if times.created >= times.expires || !files_valid {
        return Err(RepositoryError::InvalidInput);
    }
    let mut paths = HashSet::with_capacity(preview.files.len());
    for file in &preview.files {
        validate_manifest_path(&file.normalized_path)?;
        rows::validate_text(&file.version_id)?;
        if !paths.insert(file.normalized_path.as_str()) {
            return Err(RepositoryError::InvalidInput);
        }
    }
    Ok(times)
}

fn insert_preview(
    transaction: &Transaction<'_>,
    preview: &NewPreview,
    times: &PreviewTimes,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "INSERT INTO previews (id, workspace_id, project_id, capability_hash, expires_at_ms, status, created_at_ms, revoked_at_ms) SELECT ?1, ?2, p.id, ?4, ?5, 'active', ?6, NULL FROM projects p WHERE p.id = ?3 AND p.workspace_id = ?2",
            params![
                preview.id,
                preview.workspace_id,
                preview.project_id,
                preview.capability_hash,
                times.expires,
                times.created,
            ],
        )
        .map_err(map_error)?;
    changed_one_or_not_found(changed)
}

fn insert_file(
    transaction: &Transaction<'_>,
    preview: &NewPreview,
    file: &NewPreviewFile,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "INSERT INTO preview_files (preview_id, normalized_path, version_id) SELECT ?1, ?2, v.id FROM object_versions v WHERE v.id = ?3 AND v.project_id = ?4 AND v.state = 'complete'",
            params![
                preview.id,
                file.normalized_path,
                file.version_id,
                preview.project_id,
            ],
        )
        .map_err(map_error)?;
    changed_one_or_not_found(changed)
}

const fn changed_one_or_not_found(changed: usize) -> Result<(), RepositoryError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn validate_manifest_path(value: &str) -> Result<(), RepositoryError> {
    rows::validate_text(value)?;
    if is_valid_preview_path(value) {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn validate_event(
    event: &NewAuditEvent,
    action: &str,
    preview: &impl PreviewIdentity,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    let valid = event.action == action
        && event.target_type == "preview"
        && event.workspace_id == preview.workspace_id()
        && event.created_at_ms == created_at_ms
        && event.metadata
            == [(
                "previewId".to_owned(),
                blobyard_contract::AuditValue::String(preview.preview_id().to_owned()),
            )];
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

trait PreviewIdentity {
    fn preview_id(&self) -> &str;
    fn workspace_id(&self) -> &str;
}

impl PreviewIdentity for NewPreview {
    fn preview_id(&self) -> &str {
        &self.id
    }

    fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

impl PreviewIdentity for PreviewRecord {
    fn preview_id(&self) -> &str {
        &self.id
    }

    fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

#[cfg(test)]
#[path = "previews_tests.rs"]
mod tests;
