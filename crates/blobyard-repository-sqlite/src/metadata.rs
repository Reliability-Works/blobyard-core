use super::{
    SqliteRepository, collect, lifecycle_audit, map_error, migrations, rows, transfer_validation,
    validate_record,
};
use blobyard_contract::{
    AuditValue, MetadataRepository, NewAuditEvent, NewObjectVersion, ObjectVersionRecord,
    ProjectRecord, RepositoryError, StorageKey, UploadState, WorkspaceRecord,
};
use rusqlite::{Connection, Statement, params};

impl MetadataRepository for SqliteRepository {
    fn schema_version(&self) -> Result<u32, RepositoryError> {
        let connection = self.connection()?;
        migrations::schema_version(&connection)
    }

    fn create_workspace(&self, workspace: &WorkspaceRecord) -> Result<(), RepositoryError> {
        validate_record(&workspace.id, &workspace.name)?;
        self.connection()?
            .execute(
                "INSERT INTO workspaces (id, name, slug) VALUES (?1, ?2, ?3)",
                params![workspace.id, workspace.name, workspace.slug.as_str()],
            )
            .map(|_count| ())
            .map_err(map_error)
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceRecord>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id, name, slug FROM workspaces ORDER BY slug")
            .map_err(map_error)?;
        let result = query_workspaces(&mut statement);
        drop(statement);
        drop(connection);
        result
    }

    fn workspace_by_slug(
        &self,
        slug: &blobyard_core::Slug,
    ) -> Result<WorkspaceRecord, RepositoryError> {
        self.connection()?
            .query_row(
                "SELECT id, name, slug FROM workspaces WHERE slug = ?1",
                [slug.as_str()],
                rows::workspace,
            )
            .map_err(map_error)
    }

    fn rename_workspace(
        &self,
        workspace: &WorkspaceRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        validate_record(&workspace.id, &workspace.name)?;
        self.write_transaction(|transaction| {
            let previous_slug = transaction
                .query_row(
                    "SELECT slug FROM workspaces WHERE id = ?1",
                    [&workspace.id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(map_error)?;
            validate_workspace_rename_event(event, workspace, &previous_slug)?;
            let changed = transaction
                .execute(
                    "UPDATE workspaces SET name = ?2, slug = ?3 WHERE id = ?1",
                    params![workspace.id, workspace.name, workspace.slug.as_str()],
                )
                .map_err(map_error)?;
            if changed != 1 {
                return Err(RepositoryError::NotFound);
            }
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn create_project(&self, project: &ProjectRecord) -> Result<(), RepositoryError> {
        validate_record(&project.id, &project.name)?;
        rows::validate_text(&project.workspace_id)?;
        self.connection()?
            .execute(
                "INSERT INTO projects (id, workspace_id, name, slug) VALUES (?1, ?2, ?3, ?4)",
                params![
                    project.id,
                    project.workspace_id,
                    project.name,
                    project.slug.as_str()
                ],
            )
            .map(|_count| ())
            .map_err(map_error)
    }

    fn list_projects(&self, workspace_id: &str) -> Result<Vec<ProjectRecord>, RepositoryError> {
        rows::validate_text(workspace_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, workspace_id, name, slug FROM projects WHERE workspace_id = ?1 ORDER BY slug",
            )
            .map_err(map_error)?;
        let result = query_projects(&mut statement, workspace_id);
        drop(statement);
        drop(connection);
        result
    }

    fn project_by_slug(
        &self,
        workspace_id: &str,
        slug: &blobyard_core::Slug,
    ) -> Result<ProjectRecord, RepositoryError> {
        rows::validate_text(workspace_id)?;
        self.connection()?
            .query_row(
                "SELECT id, workspace_id, name, slug FROM projects WHERE workspace_id = ?1 AND slug = ?2",
                params![workspace_id, slug.as_str()],
                rows::project,
            )
            .map_err(map_error)
    }

    fn reserve_object_version(&self, version: &NewObjectVersion) -> Result<(), RepositoryError> {
        let version_number = validate_version(version)?;
        self.connection()?
            .execute(
                "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, source, git_repository, git_commit, git_branch) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    version.id,
                    version.project_id,
                    version.object_path,
                    version_number,
                    version.storage_key,
                    UploadState::Pending.as_str(),
                    version.source.as_str(),
                    version.git_repository,
                    version.git_commit,
                    version.git_branch,
                ],
            )
            .map(|_count| ())
            .map_err(map_error)
    }

    fn complete_object_version(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError> {
        transfer_validation::validate_upload_integrity(id, checksum)?;
        let connection = self.connection()?;
        transition(
            &connection,
            id,
            UploadState::Complete,
            Some(size),
            Some(checksum),
        )
    }

    fn abort_object_version(&self, id: &str) -> Result<(), RepositoryError> {
        rows::validate_text(id)?;
        let connection = self.connection()?;
        transition(&connection, id, UploadState::Aborted, None, None)
    }

    fn object_version(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        rows::validate_text(id)?;
        self.connection()?
            .query_row(
                &format!(
                    "SELECT {} FROM object_versions WHERE id = ?1",
                    rows::OBJECT_VERSION_COLUMNS
                ),
                [id],
                rows::object_version,
            )
            .map_err(map_error)
    }
}

fn validate_workspace_rename_event(
    event: &NewAuditEvent,
    workspace: &WorkspaceRecord,
    previous_slug: &str,
) -> Result<(), RepositoryError> {
    if event.action != "workspace.renamed"
        || event.target_type != "workspace"
        || event.workspace_id != workspace.id
        || event.metadata
            != [(
                "previousSlug".to_owned(),
                AuditValue::String(previous_slug.to_owned()),
            )]
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_version(version: &NewObjectVersion) -> Result<i64, RepositoryError> {
    rows::validate_text(&version.id)?;
    rows::validate_text(&version.project_id)?;
    rows::validate_text(&version.object_path)?;
    if version.version == 0 {
        return Err(RepositoryError::InvalidInput);
    }
    StorageKey::new(version.storage_key.clone()).map_err(|_error| RepositoryError::InvalidInput)?;
    transfer_validation::validate_provenance(
        version.git_repository.as_deref(),
        version.git_commit.as_deref(),
        version.git_branch.as_deref(),
    )?;
    i64::try_from(version.version).map_err(|_error| RepositoryError::InvalidInput)
}

fn query_workspaces(
    statement: &mut Statement<'_>,
) -> Result<Vec<WorkspaceRecord>, RepositoryError> {
    collect(statement.raw_query().mapped(rows::workspace))
}

pub(super) fn query_projects(
    statement: &mut Statement<'_>,
    workspace_id: &str,
) -> Result<Vec<ProjectRecord>, RepositoryError> {
    collect(
        statement
            .query_map([workspace_id], rows::project)
            .map_err(map_error)?,
    )
}

fn transition(
    connection: &Connection,
    id: &str,
    state: UploadState,
    size: Option<u64>,
    checksum: Option<&str>,
) -> Result<(), RepositoryError> {
    let size = size
        .map(|value| i64::try_from(value).map_err(|_error| RepositoryError::InvalidInput))
        .transpose()?;
    let changed = connection
        .execute(
            "UPDATE object_versions SET state = ?2, size = ?3, checksum = ?4 WHERE id = ?1 AND state = 'pending'",
            params![id, state.as_str(), size, checksum],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else if exists(connection, id)? {
        Err(RepositoryError::Conflict)
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn exists(connection: &Connection, id: &str) -> Result<bool, RepositoryError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM object_versions WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(map_error)
}
