use super::{SqliteRepository, collect, map_error, rows};
use blobyard_contract::{RepositoryError, StoredObjectRecord};
use rusqlite::{Statement, params};

pub(super) fn list(
    repository: &SqliteRepository,
    project_id: &str,
    prefix: Option<&str>,
    include_versions: bool,
) -> Result<Vec<StoredObjectRecord>, RepositoryError> {
    rows::validate_text(project_id)?;
    if let Some(value) = prefix {
        rows::validate_text(value)?;
    }
    let connection = repository.connection()?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM object_versions v JOIN upload_reservations r ON r.version_id = v.id WHERE v.project_id = ?1 AND v.state = 'complete' AND (?2 IS NULL OR substr(v.object_path, 1, length(?2)) = ?2) AND (?3 = 1 OR v.version = (SELECT MAX(latest.version) FROM object_versions latest WHERE latest.project_id = v.project_id AND latest.object_path = v.object_path AND latest.state = 'complete')) ORDER BY v.object_path, v.version",
            rows::STORED_COLUMNS
        ))
        .map_err(map_error)?;
    let result = query_objects(&mut statement, project_id, prefix, include_versions);
    drop(statement);
    drop(connection);
    result
}

fn query_objects(
    statement: &mut Statement<'_>,
    project_id: &str,
    prefix: Option<&str>,
    include_versions: bool,
) -> Result<Vec<StoredObjectRecord>, RepositoryError> {
    collect(
        statement
            .query_map(
                params![project_id, prefix, i64::from(include_versions)],
                rows::stored_object,
            )
            .map_err(map_error)?,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::query_objects;
    use blobyard_contract::RepositoryError;
    use rusqlite::Connection;

    #[test]
    fn object_query_maps_parameter_failure() {
        let connection = Connection::open_in_memory().expect("connection");
        let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
        assert_eq!(
            query_objects(&mut statement, "project", None, false).err(),
            Some(RepositoryError::Unavailable)
        );
    }
}
