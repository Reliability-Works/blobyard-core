use super::{collect, map_error, rows};
use blobyard_contract::{PreviewRecord, PreviewTarget, RepositoryError};
use rusqlite::{Connection, Statement, params};

pub(super) fn list(
    statement: &mut Statement<'_>,
    project_id: &str,
) -> Result<Vec<PreviewRecord>, RepositoryError> {
    collect(
        statement
            .query_map([project_id], rows::preview)
            .map_err(map_error)?,
    )
}

pub(super) fn by_id(
    connection: &Connection,
    preview_id: &str,
) -> Result<PreviewRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM previews WHERE id = ?1",
                rows::PREVIEW_COLUMNS
            ),
            [preview_id],
            rows::preview,
        )
        .map_err(map_error)
}

pub(super) fn target_by_capability(
    connection: &Connection,
    capability_hash: &str,
    normalized_path: &str,
    now: i64,
) -> Result<PreviewTarget, RepositoryError> {
    let preview_columns = rows::PREVIEW_COLUMNS
        .split(", ")
        .map(|column| format!("p.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        "SELECT {preview_columns}, {} FROM previews p JOIN preview_files pf ON pf.preview_id = p.id JOIN object_versions v ON v.id = pf.version_id JOIN upload_reservations r ON r.version_id = v.id WHERE p.capability_hash = ?1 AND pf.normalized_path = ?2 AND p.expires_at_ms > ?3 AND p.status = 'active' AND v.state = 'complete'",
        rows::STORED_COLUMNS,
    );
    connection
        .query_row(
            &query,
            params![capability_hash, normalized_path, now],
            |row| {
                Ok(PreviewTarget {
                    preview: rows::preview(row)?,
                    normalized_path: normalized_path.to_owned(),
                    object: rows::stored_object_from(row, 7)?,
                })
            },
        )
        .map_err(map_error)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::*;

    #[test]
    fn list_maps_parameter_binding_failures_to_unavailable() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        let mut statement = connection.prepare("SELECT 1").expect("statement");

        assert_eq!(
            list(&mut statement, "project_fixture"),
            Err(RepositoryError::Unavailable)
        );
    }
}
