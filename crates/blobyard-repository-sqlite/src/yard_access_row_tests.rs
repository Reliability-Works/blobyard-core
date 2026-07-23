use super::{grant, policy_row};
use crate::adapter::rows::tests::{assert_each_column_rejects_blob, assert_replacements_fail};
use blobyard_contract::{YardAccessGrantStatus, YardAccessPrincipalKind, YardVisibility};
use rusqlite::Connection;

const POLICY_VALUES: [&str; 4] = ["'yard_1'", "'owner'", "1", "'fixture'"];

const GRANT_VALUES: [&str; 11] = [
    "'grant_1'",
    "'yard_1'",
    "'yardenv_yard_1'",
    "'user'",
    "'user_1'",
    "'[\"editor\"]'",
    "'active'",
    "1",
    "'fixture'",
    "2",
    "NULL",
];

#[test]
fn policy_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let record = connection.query_row("SELECT 'yard_1', 'owner', 1, 'fixture'", [], policy_row)?;
    assert_eq!(record.yard_id, "yard_1");
    assert_eq!(record.visibility, YardVisibility::Owner);
    assert_eq!(record.updated_at_ms, 1);
    assert_eq!(record.updated_by_principal, "fixture");
    Ok(())
}

#[test]
fn policy_rows_reject_every_malformed_column_and_value() {
    assert_each_column_rejects_blob(&POLICY_VALUES, policy_row);
    assert_replacements_fail(&POLICY_VALUES, [(1, "'invalid'"), (2, "-1")], policy_row);
}

#[test]
fn grant_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let record = connection.query_row(
        &format!("SELECT {}", GRANT_VALUES.join(", ")),
        [],
        grant,
    )?;
    assert_eq!(record.id, "grant_1");
    assert_eq!(record.yard_id, "yard_1");
    assert_eq!(record.environment_id.as_deref(), Some("yardenv_yard_1"));
    assert_eq!(record.principal_kind, YardAccessPrincipalKind::User);
    assert_eq!(record.principal_id, "user_1");
    assert_eq!(record.app_roles, ["editor"]);
    assert_eq!(record.status, YardAccessGrantStatus::Active);
    assert_eq!(record.created_at_ms, 1);
    assert_eq!(record.created_by_principal, "fixture");
    assert_eq!(record.expires_at_ms, Some(2));
    assert_eq!(record.revoked_at_ms, None);
    Ok(())
}

#[test]
fn grant_rows_reject_every_malformed_column_and_value() {
    assert_each_column_rejects_blob(&GRANT_VALUES, grant);
    assert_replacements_fail(
        &GRANT_VALUES,
        [
            (3, "'invalid'"),
            (5, "'not json'"),
            (5, "'[1]'"),
            (5, "'[ \"editor\" ]'"),
            (6, "'invalid'"),
            (7, "-1"),
            (9, "-1"),
        ],
        grant,
    );
}
