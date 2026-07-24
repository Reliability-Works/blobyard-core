#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{continuation, listing};
use rusqlite::Connection;

#[test]
fn continuation_row_rejects_every_invalid_column_and_time() {
    let connection = Connection::open_in_memory().expect("connection");
    let base = [
        "'continuation'",
        "'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        "'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
        "'yard'",
        "'environment'",
        "'docs-fixture'",
        "'user'",
        "'/'",
        "1",
        "2",
        "NULL",
    ];
    for index in 0..base.len() {
        let mut values = base;
        values[index] = "X'00'";
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], continuation).is_err());
    }
    for (index, value) in [(8, "-1"), (9, "-1"), (10, "-1")] {
        let mut values = base;
        values[index] = value;
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], continuation).is_err());
    }
}

#[test]
fn session_listing_row_rejects_every_invalid_column_and_time() {
    let connection = Connection::open_in_memory().expect("connection");
    let base = [
        "'session'",
        "'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        "'yard'",
        "'environment'",
        "'docs-fixture'",
        "'user'",
        "1",
        "2",
        "NULL",
        "NULL",
        "'Reader'",
    ];
    for index in 0..base.len() {
        let mut values = base;
        values[index] = "X'00'";
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], listing).is_err());
    }
    for (index, value) in [(6, "-1"), (7, "-1"), (8, "-1"), (9, "-1")] {
        let mut values = base;
        values[index] = value;
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], listing).is_err());
    }
}
