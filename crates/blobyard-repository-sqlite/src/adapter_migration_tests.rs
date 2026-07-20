#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::SqliteRepository;
use blobyard_contract::RepositoryError;
use rusqlite::Connection;

fn assert_tables(repository: &SqliteRepository, tables: &[&str]) {
    let connection = repository.test_connection().expect("connection");
    for table in tables {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .expect("table query");
        assert!(exists, "{table}");
    }
    drop(connection);
}

fn legacy_upload_fixture(path: &std::path::Path, schema: u32, upload_sql: &str) {
    let mut connection = Connection::open(path).expect("legacy connection");
    super::super::migrations::apply_through(&mut connection, schema).expect("legacy schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');",
        )
        .expect("legacy namespaces");
    connection
        .execute_batch(upload_sql)
        .expect("legacy upload fixture");
}

#[test]
fn migration_version_guards_preserve_current_and_newer_databases() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    drop(SqliteRepository::open(&path).expect("new repository"));
    drop(SqliteRepository::open(&path).expect("current repository"));

    let newer = Connection::open_in_memory().expect("newer connection");
    newer
        .pragma_update(
            None,
            "user_version",
            super::super::migrations::CURRENT_SCHEMA_VERSION + 1,
        )
        .expect("newer schema version");
    assert!(matches!(
        SqliteRepository::initialize_connection(newer),
        Err(RepositoryError::SchemaTooNew)
    ));
}

#[test]
fn multipart_migration_preserves_existing_single_uploads() {
    use blobyard_contract::TransferRepository;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    legacy_upload_fixture(
        &path,
        9,
        "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, created_at_ms) VALUES ('upload', 'project', 'file.bin', 1, 'objects/upload', 'pending', 1);
         INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state) VALUES ('upload', 'upload', 'file.bin', 'application/octet-stream', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 2, 'requested');",
    );

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    let upload = repository.upload_by_id("upload").expect("existing upload");
    assert_eq!(
        upload.strategy,
        blobyard_contract::ReservationStrategy::Single
    );
    assert_eq!(upload.part_size, None);
    assert_eq!(upload.part_count, None);
    assert_eq!(upload.provider_upload_id, None);
    assert!(
        repository
            .list_upload_parts("upload")
            .expect("parts")
            .is_empty()
    );
}

#[test]
fn provider_tag_migration_preserves_uploaded_parts() {
    use blobyard_contract::TransferRepository;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    legacy_upload_fixture(
        &path,
        14,
        "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, created_at_ms) VALUES ('upload', 'project', 'file.bin', 1, 'objects/upload', 'pending', 1);
         INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state, strategy, part_size, part_count, provider_upload_id) VALUES ('upload', 'upload', 'file.bin', 'application/octet-stream', 3, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 2, 'requested', 'multipart', 3, 1, 'provider-upload');
         INSERT INTO upload_parts (upload_id, part_number, expected_size, capability_hash, expires_at_ms, state, received_size, received_checksum) VALUES ('upload', 1, 3, 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 2, 'uploaded', 3, 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd');",
    );

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    let before = repository.list_upload_parts("upload").expect("parts");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].provider_tag, None);
    repository
        .record_uploaded_part(
            "upload",
            1,
            3,
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            Some("provider-part-tag"),
        )
        .expect("provider tag");
    assert_eq!(
        repository
            .list_upload_parts("upload")
            .expect("tagged parts")[0]
            .provider_tag
            .as_deref(),
        Some("provider-part-tag")
    );
}

#[test]
fn inbox_migration_preserves_version_eleven_data_and_adds_empty_capability_tables() {
    use blobyard_contract::{InboxRepository, MetadataRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version eleven connection");
    super::super::migrations::apply_through(&mut connection, 11).expect("version eleven schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');",
        )
        .expect("version eleven fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 16);
    assert!(
        repository
            .list_inboxes("project")
            .expect("inboxes")
            .is_empty()
    );
    assert_tables(
        &repository,
        &["inboxes", "inbox_uploads", "inbox_rate_limits"],
    );
}

#[test]
fn preview_migration_preserves_version_twelve_data_and_adds_empty_manifest_tables() {
    use blobyard_contract::{InboxRepository, MetadataRepository, PreviewRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version twelve connection");
    super::super::migrations::apply_through(&mut connection, 12).expect("version twelve schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');
             INSERT INTO inboxes (id, workspace_id, project_id, name, capability_hash, expires_at_ms, status, maximum_files, maximum_bytes, created_at_ms) VALUES ('inbox', 'workspace', 'project', 'Inbox', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 2, 'active', 1, 1, 1);",
        )
        .expect("version twelve fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 16);
    assert_eq!(
        repository.list_inboxes("project").expect("inboxes").len(),
        1
    );
    assert!(
        repository
            .list_previews("project")
            .expect("previews")
            .is_empty()
    );
    assert_tables(&repository, &["previews", "preview_files"]);
}

#[test]
fn partial_migration_rejects_newer_targets_and_maps_each_database_failure() {
    assert_eq!(
        super::super::migrations::apply_through(
            &mut Connection::open_in_memory().expect("newer connection"),
            super::super::migrations::CURRENT_SCHEMA_VERSION + 1,
        ),
        Err(RepositoryError::SchemaTooNew)
    );

    let completed = (0..1_000).find(|&denied_index| {
        let mut connection = Connection::open_in_memory().expect("denied connection");
        let observed = super::install_denial(&connection, denied_index);
        let result = super::super::migrations::apply_through(&mut connection, 9);
        let count = observed.load(std::sync::atomic::Ordering::Relaxed);
        if count <= denied_index {
            result.expect("migration succeeds after every authorization point");
            true
        } else {
            assert_eq!(result, Err(RepositoryError::Unavailable));
            false
        }
    });
    assert!(completed.is_some(), "migration denial sweep must terminate");
}
