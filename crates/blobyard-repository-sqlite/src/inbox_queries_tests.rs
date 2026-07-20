#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;

const VALID: [&str; 14] = [
    "'inbox'",
    "'workspace'",
    "'project'",
    "'Fixture'",
    "5000",
    "'active'",
    "0",
    "0",
    "0",
    "0",
    "2",
    "10",
    "1000",
    "NULL",
];

fn decode(values: &[&str]) -> Result<InboxRecord, RepositoryError> {
    let connection = Connection::open_in_memory().expect("database");
    connection
        .query_row(&format!("SELECT {}", values.join(", ")), [], row)
        .map_err(map_error)
}

#[test]
fn inbox_rows_reject_every_malformed_column_status_and_timestamp() {
    assert_eq!(
        decode(&VALID).expect("valid row").status,
        InboxStatus::Active
    );
    for index in 0..VALID.len() {
        let mut values = VALID;
        values[index] = "x'00'";
        assert_eq!(decode(&values), Err(RepositoryError::Unavailable));
    }
    let mut invalid_status = VALID;
    invalid_status[5] = "'invalid'";
    assert_eq!(decode(&invalid_status), Err(RepositoryError::Unavailable));
    for index in [4, 6, 7, 8, 9, 10, 11, 12, 13] {
        let mut values = VALID;
        values[index] = "-1";
        assert_eq!(decode(&values), Err(RepositoryError::Unavailable));
    }
}

#[test]
fn inbox_list_and_upload_queries_map_statement_failures() {
    let connection = Connection::open_in_memory().expect("database");
    let mut statement = connection.prepare("SELECT 1").expect("statement");
    assert_eq!(
        list(&mut statement, "project"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        upload(&connection, &"a".repeat(64), "upload", 1),
        Err(RepositoryError::Unavailable)
    );
}
