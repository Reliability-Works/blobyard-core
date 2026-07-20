use super::{deploy, yard};
use crate::adapter::rows::tests::{assert_each_column_rejects_blob, assert_replacements_fail};
use blobyard_contract::{WebYardStatus, YardDeployStatus};
use rusqlite::Connection;

const YARD_VALUES: [&str; 10] = [
    "'yard_1'",
    "'workspace_1'",
    "'project_1'",
    "'docs'",
    "'docs-123456789-team'",
    "'deploy_1'",
    "'active'",
    "1",
    "2",
    "NULL",
];

const DEPLOY_VALUES: [&str; 14] = [
    "'deploy_1'",
    "'yard_1'",
    "'workspace_1'",
    "'project_1'",
    "'client_identifier1'",
    "'.blobyard-yard/yard_1/client_identifier1/'",
    "'docs-0123456789-team'",
    "1",
    "0",
    "'live'",
    "1",
    "2",
    "3",
    "4",
];

#[test]
fn yard_and_deploy_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let yard = connection
        .query_row(
            "SELECT 'yard_1', 'workspace_1', 'project_1', 'docs', 'docs-123456789-team', 'deploy_1', 'active', 1, 2, NULL",
            [],
            yard,
        )?;
    assert_eq!(yard.status, WebYardStatus::Active);
    assert_eq!(yard.current_deploy_id.as_deref(), Some("deploy_1"));

    let deploy = connection
        .query_row(
            "SELECT 'deploy_1', 'yard_1', 'workspace_1', 'project_1', 'client_identifier1', '.blobyard-yard/yard_1/client_identifier1/', 'docs-0123456789-team', 1, 0, 'live', 1, 2, 3, 4",
            [],
            deploy,
        )?;
    assert_eq!(deploy.status, YardDeployStatus::Live);
    assert!(deploy.spa);
    assert!(!deploy.clean_urls);
    assert_eq!(deploy.file_count, 3);
    Ok(())
}

#[test]
fn yard_rows_reject_invalid_provider_values() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    for query in [
        "SELECT 'yard_1', 'workspace_1', 'project_1', 'invalid slug', 'host', NULL, 'active', 1, 1, NULL",
        "SELECT 'yard_1', 'workspace_1', 'project_1', 'docs', 'host', NULL, 'invalid', 1, 1, NULL",
        "SELECT 'yard_1', 'workspace_1', 'project_1', 'docs', 'host', NULL, 'active', -1, 1, NULL",
    ] {
        assert!(connection.query_row(query, [], yard).is_err());
    }
    for query in [
        "SELECT 'deploy_1', 'yard_1', 'workspace_1', 'project_1', 'client_identifier1', 'root', 'host', 0, 0, 'invalid', 1, NULL, 0, 0",
        "SELECT 'deploy_1', 'yard_1', 'workspace_1', 'project_1', 'client_identifier1', 'root', 'host', 0, 0, 'uploading', -1, NULL, 0, 0",
    ] {
        assert!(connection.query_row(query, [], deploy).is_err());
    }
    Ok(())
}

#[test]
fn yard_and_deploy_rows_reject_every_malformed_column_and_timestamp() {
    assert_each_column_rejects_blob(&YARD_VALUES, yard);
    assert_replacements_fail(
        &YARD_VALUES,
        [
            (3, "'invalid slug'"),
            (6, "'invalid'"),
            (7, "-1"),
            (8, "-1"),
            (9, "-1"),
        ],
        yard,
    );
    assert_each_column_rejects_blob(&DEPLOY_VALUES, deploy);
    assert_replacements_fail(
        &DEPLOY_VALUES,
        [
            (9, "'invalid'"),
            (10, "-1"),
            (11, "-1"),
            (12, "-1"),
            (13, "-1"),
        ],
        deploy,
    );
}
