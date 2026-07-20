use super::{collect, map_error, rows, yard_rows};
use blobyard_contract::{RepositoryError, WebYardRecord, YardDeployRecord, YardFileTarget};
use rusqlite::{Connection, OptionalExtension, Statement, params};

pub(super) fn yard_by_id(
    connection: &Connection,
    yard_id: &str,
) -> Result<WebYardRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM web_yards WHERE id = ?1",
                yard_rows::YARD_COLUMNS
            ),
            [yard_id],
            yard_rows::yard,
        )
        .map_err(map_error)
}

pub(super) fn deploy_by_id(
    connection: &Connection,
    deploy_id: &str,
) -> Result<YardDeployRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM yard_deploys WHERE id = ?1",
                yard_rows::DEPLOY_COLUMNS
            ),
            [deploy_id],
            yard_rows::deploy,
        )
        .map_err(map_error)
}

pub(super) fn active_named_yard(
    connection: &Connection,
    project_id: &str,
    name: &str,
) -> Result<Option<WebYardRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM web_yards WHERE project_id = ?1 AND name = ?2 AND status != 'deleted'",
                yard_rows::YARD_COLUMNS
            ),
            params![project_id, name],
            yard_rows::yard,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn deploy_by_client_id(
    connection: &Connection,
    client_deploy_id: &str,
) -> Result<Option<YardDeployRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM yard_deploys WHERE client_deploy_id = ?1",
                yard_rows::DEPLOY_COLUMNS
            ),
            [client_deploy_id],
            yard_rows::deploy,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn list_yards(
    statement: &mut Statement<'_>,
    project_id: &str,
) -> Result<Vec<WebYardRecord>, RepositoryError> {
    collect(
        statement
            .query_map([project_id], yard_rows::yard)
            .map_err(map_error)?,
    )
}

pub(super) fn list_deploys(
    statement: &mut Statement<'_>,
    yard_id: &str,
) -> Result<Vec<YardDeployRecord>, RepositoryError> {
    collect(
        statement
            .query_map([yard_id], yard_rows::deploy)
            .map_err(map_error)?,
    )
}

pub(super) fn public_file(
    connection: &Connection,
    host_label: &str,
    normalized_request_path: &str,
) -> Result<YardFileTarget, RepositoryError> {
    let deploy = serving_deploy(connection, host_label)?;
    for (path, not_found_document) in resolution_candidates(normalized_request_path, &deploy) {
        if let Some(object) = file_by_path(connection, &deploy.id, &path)? {
            return Ok(YardFileTarget {
                object,
                not_found_document,
            });
        }
    }
    Err(RepositoryError::NotFound)
}

fn serving_deploy(
    connection: &Connection,
    host_label: &str,
) -> Result<YardDeployRecord, RepositoryError> {
    if let Some(deploy) = stable_deploy(connection, host_label)? {
        return Ok(deploy);
    }
    immutable_deploy(connection, host_label)?.ok_or(RepositoryError::NotFound)
}

fn stable_deploy(
    connection: &Connection,
    host_label: &str,
) -> Result<Option<YardDeployRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM web_yards y JOIN yard_deploys d ON d.id = y.current_deploy_id WHERE y.host_label = ?1 AND y.status = 'active' AND d.yard_id = y.id AND d.status = 'live'",
                yard_rows::QUALIFIED_DEPLOY_COLUMNS
            ),
            [host_label],
            yard_rows::deploy,
        )
        .optional()
        .map_err(map_error)
}

fn immutable_deploy(
    connection: &Connection,
    host_label: &str,
) -> Result<Option<YardDeployRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM yard_deploys d JOIN web_yards y ON y.id = d.yard_id WHERE d.deployment_host_label = ?1 AND y.status = 'active' AND d.status IN ('live', 'superseded')",
                yard_rows::QUALIFIED_DEPLOY_COLUMNS
            ),
            [host_label],
            yard_rows::deploy,
        )
        .optional()
        .map_err(map_error)
}

fn resolution_candidates(
    normalized_request_path: &str,
    deploy: &YardDeployRecord,
) -> Vec<(String, bool)> {
    let trimmed = normalized_request_path
        .strip_suffix('/')
        .unwrap_or(normalized_request_path);
    let mut candidates = Vec::with_capacity(5);
    if normalized_request_path.is_empty() {
        candidates.push(("index.html".to_owned(), false));
    } else {
        if normalized_request_path == trimmed {
            candidates.push((trimmed.to_owned(), false));
        }
        candidates.push((format!("{trimmed}/index.html"), false));
        if deploy.clean_urls && extensionless(trimmed) {
            candidates.push((format!("{trimmed}.html"), false));
        }
        if deploy.spa && extensionless(trimmed) {
            candidates.push(("index.html".to_owned(), false));
        }
    }
    candidates.push(("404.html".to_owned(), true));
    candidates
}

fn extensionless(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| !name.contains('.'))
}

fn file_by_path(
    connection: &Connection,
    deploy_id: &str,
    normalized_path: &str,
) -> Result<Option<blobyard_contract::StoredObjectRecord>, RepositoryError> {
    let query = format!(
        "SELECT {} FROM yard_deploy_files f JOIN object_versions v ON v.id = f.version_id JOIN upload_reservations r ON r.version_id = v.id WHERE f.deploy_id = ?1 AND f.normalized_path = ?2 AND v.state = 'complete'",
        rows::STORED_COLUMNS
    );
    connection
        .query_row(
            &query,
            params![deploy_id, normalized_path],
            rows::stored_object,
        )
        .optional()
        .map_err(map_error)
}
