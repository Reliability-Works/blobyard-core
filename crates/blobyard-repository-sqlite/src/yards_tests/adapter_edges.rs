use super::{
    super::{SqliteRepository, yard_queries},
    success,
    transaction_edges::support::{created, deploy, deployed, yard},
};
use blobyard_contract::{NewYardFile, RepositoryError, WebYardRepository};
use rusqlite::Connection;

#[test]
fn public_yard_adapter_rejects_invalid_optional_ids_and_request_paths() {
    let temporary = success(tempfile::tempdir());
    let repository = success(SqliteRepository::open(
        &temporary.path().join("metadata.sqlite3"),
    ));
    assert_eq!(
        repository.rollback_web_yard("yard_fixture", Some(""), 1, &event()),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.yard_file_by_host("host-fixture", "../unsafe"),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn public_yard_adapter_rejects_invalid_required_ids_before_database_access() {
    let temporary = success(tempfile::tempdir());
    let repository = success(SqliteRepository::open(
        &temporary.path().join("metadata.sqlite3"),
    ));
    let candidate_yard = yard("invalid", 1);
    let candidate_deploy = deploy(&candidate_yard, 1, false);
    let files = [NewYardFile {
        normalized_path: "index.html".to_owned(),
        version_id: "version_fixture".to_owned(),
        byte_size: 1,
    }];
    assert_eq!(
        repository.list_web_yards(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.web_yard_by_id(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_yard_deploys(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.yard_deploy_by_id(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.finalise_yard_deploy("", &files, 2, &deployed("", 1, 1, "live", 2),),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.fail_yard_deploy("", "UPLOAD_FAILED", "Failed.", 2),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.rollback_web_yard("", None, 2, &event()),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.delete_web_yard("", 2, &created("", 2)),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.yard_file_by_host("", ""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.pending_yard_cleanups(Some("")),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid = candidate_yard;
    invalid.id.clear();
    assert_eq!(
        repository.start_yard_deploy(&invalid, &candidate_deploy, &created("", 1)),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn public_yard_adapter_lists_all_pending_cleanups_without_a_yard_filter() {
    let temporary = success(tempfile::tempdir());
    let repository = success(SqliteRepository::open(
        &temporary.path().join("metadata.sqlite3"),
    ));

    assert_eq!(repository.pending_yard_cleanups(None), Ok(Vec::new()));
}

#[test]
fn yard_query_collectors_propagate_parameter_binding_failures() {
    let connection = success(Connection::open_in_memory());
    let mut yards = success(connection.prepare(
        "SELECT 'yard', 'workspace', 'project', 'docs', 'docs-123456789-team', NULL, 'active', 1, 1, NULL",
    ));
    assert_eq!(
        yard_queries::list_yards(&mut yards, "project"),
        Err(RepositoryError::Unavailable)
    );
    let mut deploys = success(connection.prepare(
        "SELECT 'deploy', 'yard', 'workspace', 'project', 'client_identifier1', 'root', 'docs-0123456789-team', 0, 0, 'uploading', 1, NULL, 0, 0",
    ));
    assert_eq!(
        yard_queries::list_deploys(&mut deploys, "yard"),
        Err(RepositoryError::Unavailable)
    );
}

fn event() -> blobyard_contract::NewAuditEvent {
    blobyard_contract::NewAuditEvent {
        id: "audit_yard_edge".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "yard.rolled_back".to_owned(),
        request_id: "request_yard_edge".to_owned(),
        target_type: "yard_deploy".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    }
}
