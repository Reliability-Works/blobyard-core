#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use rusqlite::Connection;

fn group_row(expression: &str) -> rusqlite::Result<blobyard_contract::WorkspaceGroupRecord> {
    let connection = Connection::open_in_memory().expect("connection");
    connection.query_row(
        &format!(
            "SELECT {expression}
             FROM (SELECT 1)"
        ),
        [],
        super::super::workspace_group_rows::group,
    )
}

fn member_row(expression: &str) -> rusqlite::Result<blobyard_contract::WorkspaceGroupMemberRecord> {
    let connection = Connection::open_in_memory().expect("connection");
    connection.query_row(
        &format!(
            "SELECT {expression}
             FROM (SELECT 1)"
        ),
        [],
        super::super::workspace_group_rows::member,
    )
}

#[test]
fn group_row_rejects_every_malformed_column() {
    let valid = "'group_00000000000000000000000000000001',
        'workspace_fixture', 'Reviewers', 'active', 0, 1, NULL";
    assert!(group_row(valid).is_ok());
    for malformed in [
        "x'01', 'workspace_fixture', 'Reviewers', 'active', 0, 1, NULL",
        "'group_00000000000000000000000000000001', x'01', 'Reviewers', 'active', 0, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', x'01', 'active', 0, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', x'01', 0, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'unknown', 0, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 'zero', 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', -1, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 4294967296, 1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 0, 'one', NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 0, -1, NULL",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 0, 1, 'two'",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'Reviewers', 'active', 0, 1, -1",
    ] {
        assert!(group_row(malformed).is_err(), "{malformed}");
    }
}

#[test]
fn member_row_rejects_every_malformed_column() {
    let valid = "'group_00000000000000000000000000000001',
        'workspace_fixture', 'user_fixture', 1";
    assert!(member_row(valid).is_ok());
    for malformed in [
        "x'01', 'workspace_fixture', 'user_fixture', 1",
        "'group_00000000000000000000000000000001', x'01', 'user_fixture', 1",
        "'group_00000000000000000000000000000001', 'workspace_fixture', x'01', 1",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'user_fixture', 'one'",
        "'group_00000000000000000000000000000001', 'workspace_fixture', 'user_fixture', -1",
    ] {
        assert!(member_row(malformed).is_err(), "{malformed}");
    }
}
