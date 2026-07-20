use blobyard_core::hex_digest;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) fn install_scoped_token(data_directory: &Path, scopes: &[&str]) -> String {
    let raw = format!("test_{}", uuid::Uuid::new_v4().simple());
    let hash = hex_digest(&Sha256::digest(raw.as_bytes()));
    let encoded = serde_json::to_string(scopes).expect("scope JSON");
    connection(data_directory)
        .execute(
            "INSERT INTO api_tokens (id, name, secret_hash, scopes, workspace_id, revoked) VALUES (?1, 'Scoped test', ?2, ?3, 'workspace_default', 0)",
            rusqlite::params![format!("token_{}", uuid::Uuid::new_v4().simple()), hash, encoded],
        )
        .expect("scoped token");
    raw
}

pub(crate) fn insert_audit_events(data_directory: &Path, count: u64) {
    let mut connection = connection(data_directory);
    let transaction = connection.transaction().expect("audit transaction");
    for index in 0..count {
        transaction
            .execute(
                "INSERT INTO audit_events (id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms) VALUES (?1, 'workspace_default', 'token_fixture', 'fixture.recorded', ?2, 'fixture', ?3, ?4)",
                rusqlite::params![
                    format!("audit_fixture_{index}"),
                    format!("request_fixture_{index}"),
                    format!(r#"{{"bool":true,"null":null,"number":{index},"text":"safe"}}"#),
                    i64::try_from(index).expect("audit time"),
                ],
            )
            .expect("audit fixture");
    }
    transaction.commit().expect("audit commit");
}

pub(crate) fn project_id(data_directory: &Path) -> String {
    connection(data_directory)
        .query_row(
            "SELECT id FROM projects WHERE slug = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("project ID")
}

pub(crate) fn storage_records(data_directory: &Path, object_path: &str) -> Vec<(String, String)> {
    let connection = connection(data_directory);
    let mut statement = connection
        .prepare("SELECT id, storage_key FROM object_versions WHERE object_path = ?1 ORDER BY id")
        .expect("storage query");
    statement
        .query_map([object_path], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("storage records")
        .collect::<Result<Vec<_>, _>>()
        .expect("storage rows")
}

pub(crate) fn corrupt_object_as_directory(data_directory: &Path, storage_key: &str) {
    let path = object_path(data_directory, storage_key);
    std::fs::remove_file(&path).expect("remove stored object");
    std::fs::create_dir(&path).expect("replace object with directory");
}

pub(crate) fn remove_corruption(data_directory: &Path, storage_key: &str) {
    let object = object_path(data_directory, storage_key);
    std::fs::remove_dir(&object).expect("remove corrupt directory");
    let metadata = metadata_path(data_directory, storage_key);
    std::fs::remove_file(metadata).expect("remove stale metadata");
}

fn connection(data_directory: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(data_directory.join("metadata.sqlite3")).expect("metadata database")
}

fn object_path(data_directory: &Path, storage_key: &str) -> PathBuf {
    data_directory
        .join("objects")
        .join("objects")
        .join(storage_key)
}

fn metadata_path(data_directory: &Path, storage_key: &str) -> PathBuf {
    let mut path = data_directory
        .join("objects")
        .join("metadata")
        .join(storage_key);
    path.set_extension(format!(
        "{}blobyard-meta",
        path.extension()
            .map_or_else(String::new, |value| format!("{}.", value.to_string_lossy()))
    ));
    path
}
