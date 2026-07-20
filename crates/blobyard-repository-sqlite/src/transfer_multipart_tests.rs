#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;

#[test]
fn multipart_query_maps_an_execution_interrupt_after_preparation() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut wrong_statement = connection.prepare("SELECT 1").expect("wrong statement");
    assert_eq!(
        query_parts(&mut wrong_statement, "upload_invalid_parameters"),
        Err(RepositoryError::Unavailable)
    );
    drop(wrong_statement);
    connection
        .execute_batch(
            "CREATE TABLE upload_parts (
                upload_id TEXT NOT NULL,
                part_number INTEGER NOT NULL,
                expected_size INTEGER NOT NULL,
                expires_at_ms INTEGER NOT NULL,
                received_size INTEGER,
                received_checksum TEXT,
                provider_tag TEXT
            );",
        )
        .expect("schema");
    let mut statement = connection
        .prepare(&format!(
            "SELECT {PART_COLUMNS} FROM upload_parts p WHERE p.upload_id = ?1 ORDER BY p.part_number"
        ))
        .expect("statement");
    connection
        .progress_handler(1, Some(|| true))
        .expect("progress handler");
    assert_eq!(
        query_parts(&mut statement, "upload_interrupted"),
        Err(RepositoryError::Unavailable)
    );
}
