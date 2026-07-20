//! Hosted-to-standalone migration acceptance over hosted-compatible HTTP fixtures.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

include!("hosted_migration/fixture.rs");

use blobyard_contract::{
    LifecycleRepository, MetadataRepository, ObjectStorage, SharingRepository, StorageKey,
    TransferRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_server::{HostedMigrationError, migrate_from_hosted};
use blobyard_storage_filesystem::FilesystemStorage;
use std::io::Read;
use std::io::Write;
use std::process::{Command, Stdio};

#[tokio::test]
async fn hosted_fixture_migrates_with_byte_version_checksum_and_policy_equivalence() {
    let fixture = spawn("source", StatusCode::OK).await;
    let temporary = tempfile::tempdir().expect("destination parent");
    let destination = temporary.path().join("standalone");
    let report = migrate_from_hosted(
        &options(fixture.origin.clone(), destination.clone(), "source"),
        source_token(),
    )
    .await
    .expect("hosted migration");

    let report: Value = serde_json::from_str(&report).expect("report JSON");
    assert_eq!(report["format"], "Blob Yard hosted migration v1");
    assert_eq!(report["workspaceCount"], 1);
    assert_eq!(report["projectCount"], 1);
    assert_eq!(report["objectVersionCount"], 1);
    assert_eq!(report["sharePolicyCount"], 1);
    assert_eq!(report["retentionPolicyCount"], 1);
    assert!(
        report["bootstrapToken"]
            .as_str()
            .expect("token")
            .starts_with("byb_")
    );
    assert!(
        report["shareUrls"][0]
            .as_str()
            .expect("share URL")
            .starts_with("http://127.0.0.1:8787/s/bysh_")
    );

    assert_equivalent_destination(&destination, &fixture.object);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_migration_command_reads_stdin_and_prints_the_verified_report() {
    let fixture = spawn("source", StatusCode::OK).await;
    let temporary = tempfile::tempdir().expect("destination parent");
    let destination = temporary.path().join("command-standalone");
    let origin = fixture.origin.clone();
    let command_destination = destination.clone();
    let output = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_blobyard-server"))
            .args([
                "hosted-migrate",
                "--source-url",
                &origin,
                "--token-stdin",
                "--workspace",
                "source",
                "--data-dir",
                command_destination.to_str().expect("destination path"),
                "--public-url",
                "http://127.0.0.1:8787",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("hosted migration command");
        child
            .stdin
            .take()
            .expect("command stdin")
            .write_all(b"byd_pat_fixture\r\n")
            .expect("source token");
        child.wait_with_output().expect("command output")
    })
    .await
    .expect("command task");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).expect("report JSON");
    assert_eq!(report["workspaceCount"], 1);
    assert_eq!(report["objectVersionCount"], 1);
    assert_equivalent_destination(&destination, &fixture.object);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_migration_command_propagates_a_closed_report_stream() {
    let fixture = spawn("source", StatusCode::OK).await;
    let temporary = tempfile::tempdir().expect("destination parent");
    let destination = temporary.path().join("closed-report");
    let origin = fixture.origin.clone();
    let status = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_blobyard-server"))
            .args([
                "hosted-migrate",
                "--source-url",
                &origin,
                "--token-stdin",
                "--workspace",
                "source",
                "--data-dir",
                destination.to_str().expect("destination path"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("hosted migration command");
        drop(child.stdout.take());
        child
            .stdin
            .take()
            .expect("command stdin")
            .write_all(b"byd_pat_fixture\n")
            .expect("source token");
        child.wait().expect("command status")
    })
    .await
    .expect("command task");
    assert!(!status.success());
}

#[tokio::test]
async fn public_migration_rejects_invalid_options_and_unavailable_source() {
    let temporary = tempfile::tempdir().expect("destination parent");
    let mut invalid = options(
        "http://127.0.0.1:1".to_owned(),
        temporary.path().join("invalid"),
        "source",
    );
    invalid.poll_limit = 0;
    assert_eq!(
        migrate_from_hosted(&invalid, source_token()).await.err(),
        Some(HostedMigrationError::InvalidInput)
    );

    let unavailable = options(
        "http://127.0.0.1:1".to_owned(),
        temporary.path().join("unavailable"),
        "source",
    );
    assert_eq!(
        migrate_from_hosted(&unavailable, source_token())
            .await
            .err(),
        Some(HostedMigrationError::SourceApi)
    );
}

#[tokio::test]
async fn public_migration_propagates_projection_object_and_destination_failures() {
    let temporary = tempfile::tempdir().expect("destination parent");
    let invalid_projection = spawn("INVALID", StatusCode::OK).await;
    assert_eq!(
        migrate_from_hosted(
            &options(
                invalid_projection.origin.clone(),
                temporary.path().join("projection"),
                "INVALID",
            ),
            source_token(),
        )
        .await
        .err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let failed_object = spawn("source", StatusCode::INTERNAL_SERVER_ERROR).await;
    assert_eq!(
        migrate_from_hosted(
            &options(
                failed_object.origin.clone(),
                temporary.path().join("object"),
                "source",
            ),
            source_token(),
        )
        .await
        .err(),
        Some(HostedMigrationError::SourceDownload)
    );

    let occupied = spawn("source", StatusCode::OK).await;
    let occupied_destination = temporary.path().join("occupied");
    std::fs::create_dir(&occupied_destination).expect("occupied destination");
    assert_eq!(
        migrate_from_hosted(
            &options(occupied.origin.clone(), occupied_destination, "source"),
            source_token(),
        )
        .await
        .err(),
        Some(HostedMigrationError::DestinationExists)
    );
}

fn assert_equivalent_destination(destination: &std::path::Path, expected_bytes: &[u8]) {
    let repository =
        SqliteRepository::open(&destination.join("metadata.sqlite3")).expect("repository");
    let storage_key = assert_equivalent_metadata(&repository, expected_bytes);
    let storage = FilesystemStorage::open(&destination.join("objects")).expect("storage");
    let mut read = storage
        .get(&StorageKey::new(storage_key).expect("storage key"), None)
        .expect("stored bytes");
    let mut bytes = Vec::new();
    read.reader.read_to_end(&mut bytes).expect("read bytes");
    assert_eq!(bytes, expected_bytes);
}

fn assert_equivalent_metadata(repository: &SqliteRepository, expected_bytes: &[u8]) -> String {
    let workspace = repository.list_workspaces().expect("workspaces");
    assert_eq!(workspace[0].id, "workspace_default");
    assert_eq!(workspace[0].slug.as_str(), "source");
    let project = repository
        .list_projects("workspace_default")
        .expect("projects")
        .remove(0);
    let object = repository
        .list_stored_objects(&project.id, None, true)
        .expect("objects")
        .remove(0);
    assert_eq!(object.version.version, 7);
    assert_eq!(object.version.size, Some(expected_bytes.len() as u64));
    assert_eq!(
        object.version.checksum.as_deref(),
        Some(checksum(expected_bytes).as_str())
    );
    assert_eq!(object.filename, "app.zip");
    let shares = repository.list_shares("workspace_default").expect("shares");
    assert_eq!(shares[0].consumed_count, 1);
    assert_eq!(shares[0].maximum_downloads, Some(3));
    let retention = repository.retention_policy(&project.id).expect("retention");
    assert_eq!(retention.keep_latest, 4);
    assert_eq!(retention.path_glob.as_deref(), Some("releases/**"));
    assert_eq!(retention.branch_glob.as_deref(), Some("main"));
    object.version.storage_key
}
