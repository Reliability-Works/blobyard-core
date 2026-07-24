#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::admission;
use rusqlite::Connection;

#[test]
fn admission_row_rejects_each_non_text_column() {
    let connection = Connection::open_in_memory().expect("connection");
    let base = ["'yard'", "'environment'", "'workspace'"];
    for index in 0..base.len() {
        let mut values = base;
        values[index] = "X'00'";
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], admission).is_err());
    }
}
