use super::{listing_row, user_row};
use crate::adapter::rows::tests::{assert_each_column_rejects_blob, assert_replacements_fail};
use blobyard_contract::LocalUserStatus;
use rusqlite::Connection;

const USER_VALUES: [&str; 7] = [
    "'user_1'",
    "'workspace_1'",
    "'Ada'",
    "'ada@example.test'",
    "'active'",
    "1",
    "NULL",
];

const LISTING_VALUES: [&str; 8] = [
    "'user_1'",
    "'workspace_1'",
    "'Ada'",
    "'ada@example.test'",
    "'active'",
    "1",
    "NULL",
    "'byuk_a'",
];

#[test]
fn user_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let record =
        connection.query_row(&format!("SELECT {}", USER_VALUES.join(", ")), [], user_row)?;
    assert_eq!(record.id, "user_1");
    assert_eq!(record.workspace_id, "workspace_1");
    assert_eq!(record.display_name, "Ada");
    assert_eq!(record.email.as_deref(), Some("ada@example.test"));
    assert_eq!(record.status, LocalUserStatus::Active);
    assert_eq!(record.created_at_ms, 1);
    assert_eq!(record.deactivated_at_ms, None);
    let tombstoned = connection.query_row(
        "SELECT 'user_2', 'workspace_1', 'Bea', NULL, 'deactivated', 1, 2",
        [],
        user_row,
    )?;
    assert_eq!(tombstoned.email, None);
    assert_eq!(tombstoned.status, LocalUserStatus::Deactivated);
    assert_eq!(tombstoned.deactivated_at_ms, Some(2));
    Ok(())
}

#[test]
fn user_rows_reject_every_malformed_column_and_value() {
    assert_each_column_rejects_blob(&USER_VALUES, user_row);
    assert_replacements_fail(
        &USER_VALUES,
        [(4, "'invalid'"), (5, "-1"), (6, "-1")],
        user_row,
    );
}

#[test]
fn listing_rows_decode_complete_records() -> rusqlite::Result<()> {
    let connection = Connection::open_in_memory()?;
    let listing = connection.query_row(
        &format!("SELECT {}", LISTING_VALUES.join(", ")),
        [],
        listing_row,
    )?;
    assert_eq!(listing.user.id, "user_1");
    assert_eq!(listing.active_key_prefix.as_deref(), Some("byuk_a"));
    let bare = connection.query_row(
        "SELECT 'user_2', 'workspace_1', 'Bea', NULL, 'active', 1, NULL, NULL",
        [],
        listing_row,
    )?;
    assert_eq!(bare.active_key_prefix, None);
    Ok(())
}

#[test]
fn listing_rows_reject_every_malformed_column_and_value() {
    assert_each_column_rejects_blob(&LISTING_VALUES, listing_row);
    assert_replacements_fail(&LISTING_VALUES, [(4, "'invalid'"), (5, "-1")], listing_row);
}
