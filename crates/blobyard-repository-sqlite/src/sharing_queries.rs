use super::{collect, map_error, rows};
use blobyard_contract::{RepositoryError, ShareRecord, ShareTarget};
use rusqlite::{Connection, OptionalExtension, Statement, params};

pub(super) fn list(
    statement: &mut Statement<'_>,
    workspace_id: &str,
) -> Result<Vec<ShareRecord>, RepositoryError> {
    collect(
        statement
            .query_map([workspace_id], rows::share)
            .map_err(map_error)?,
    )
}

pub(super) fn share_by_id(
    connection: &Connection,
    share_id: &str,
) -> Result<ShareRecord, RepositoryError> {
    connection
        .query_row(
            &format!("SELECT {} FROM shares WHERE id = ?1", rows::SHARE_COLUMNS),
            [share_id],
            rows::share,
        )
        .map_err(map_error)
}

pub(super) fn target_by_capability(
    connection: &Connection,
    capability_hash: &str,
    now: i64,
    require_download: bool,
) -> Result<ShareTarget, RepositoryError> {
    let state = if require_download {
        "s.status = 'active'"
    } else {
        "s.status IN ('active', 'exhausted')"
    };
    let query = format!(
        "SELECT {}, {} FROM shares s JOIN object_versions v ON v.id = s.version_id JOIN upload_reservations r ON r.version_id = v.id WHERE s.capability_hash = ?1 AND s.expires_at_ms > ?2 AND {state} AND v.state = 'complete'",
        rows::SHARE_COLUMNS
            .split(", ")
            .map(|column| format!("s.{column}"))
            .collect::<Vec<_>>()
            .join(", "),
        rows::STORED_COLUMNS,
    );
    connection
        .query_row(&query, params![capability_hash, now], |row| {
            Ok(ShareTarget {
                share: rows::share(row)?,
                object: rows::stored_object_from(row, 9)?,
            })
        })
        .optional()
        .map_err(map_error)?
        .ok_or(RepositoryError::NotFound)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::list;
    use blobyard_contract::RepositoryError;
    use rusqlite::Connection;

    #[test]
    fn share_list_maps_parameter_binding_failure() {
        let connection = Connection::open_in_memory().expect("connection");
        let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
        assert_eq!(
            list(&mut statement, "workspace").err(),
            Some(RepositoryError::Unavailable)
        );
    }
}
