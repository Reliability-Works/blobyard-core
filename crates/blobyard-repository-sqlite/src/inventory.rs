use super::{SqliteRepository, collect, map_error, rows};
use blobyard_contract::{MetadataRepositoryInventory, ObjectVersionRecord, RepositoryError};
use rusqlite::Statement;

impl MetadataRepositoryInventory for SqliteRepository {
    fn list_object_versions(&self) -> Result<Vec<ObjectVersionRecord>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM object_versions ORDER BY storage_key, id",
                rows::OBJECT_VERSION_COLUMNS
            ))
            .map_err(map_error)?;
        let result = query_versions(&mut statement);
        drop(statement);
        drop(connection);
        result
    }
}

pub(super) fn query_versions(
    statement: &mut Statement<'_>,
) -> Result<Vec<ObjectVersionRecord>, RepositoryError> {
    let rows = statement
        .query_map([], rows::object_version)
        .map_err(map_error)?;
    collect(rows)
}
