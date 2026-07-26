use super::{deploy, environment, yard};
use crate::adapter::rows::tests::{assert_each_column_rejects_blob, assert_replacements_fail};
use blobyard_contract::{
    WebYardStatus, YardDeployStatus, YardEnvironmentKind, YardEnvironmentStatus,
};
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

const ENVIRONMENT_VALUES: [&str; 7] = [
    "'yardenv_yard_1'",
    "'yard_1'",
    "'production'",
    "'production'",
    "'active'",
    "1",
    "2",
];

#[test]
fn environment_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let record = connection.query_row(
        "SELECT 'yardenv_yard_1', 'yard_1', 'production', 'production', 'active', 1, 2",
        [],
        environment,
    )?;
    assert_eq!(record.id, "yardenv_yard_1");
    assert_eq!(record.yard_id, "yard_1");
    assert_eq!(record.name.as_str(), "production");
    assert_eq!(record.kind, YardEnvironmentKind::Production);
    assert_eq!(record.status, YardEnvironmentStatus::Active);
    assert_eq!(record.created_at_ms, 1);
    assert_eq!(record.updated_at_ms, 2);
    Ok(())
}

#[test]
fn environment_rows_reject_every_malformed_column_and_timestamp() {
    assert_each_column_rejects_blob(&ENVIRONMENT_VALUES, environment);
    assert_replacements_fail(
        &ENVIRONMENT_VALUES,
        [
            (2, "'invalid slug'"),
            (3, "'invalid'"),
            (4, "'invalid'"),
            (5, "-1"),
            (6, "-1"),
        ],
        environment,
    );
}

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
