use super::*;

fn open_repository() -> (tempfile::TempDir, SqliteRepository) {
    super::empty_repository()
}

fn seed_project(repository: &SqliteRepository) {
    repository
        .create_workspace(&workspace())
        .expect("workspace");
    repository.create_project(&project()).expect("project");
}

fn seed_complete_version(repository: &SqliteRepository) {
    seed_project(repository);
    repository
        .reserve_object_version(&version())
        .expect("version");
    repository
        .complete_object_version("version_fixture", 1, &checksum('a'))
        .expect("complete version");
}

fn repository_with_retention() -> (tempfile::TempDir, SqliteRepository) {
    let (temporary, repository) = open_repository();
    seed_project(&repository);
    repository
        .set_retention(&policy(), &event("retention.policy_set"))
        .expect("retention policy");
    (temporary, repository)
}

fn event(action: &str) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("event_{}", action.replace('.', "_")),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "project".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 1,
    }
}

#[test]
fn lifecycle_timestamps_fail_at_their_durable_write_boundaries() {
    deletion_timestamps_fail_closed();
    retention_policy_timestamps_fail_closed();
    retention_run_timestamps_fail_closed();
}

fn deletion_timestamps_fail_closed() {
    let (_temporary, repository) = open_repository();
    seed_complete_version(&repository);
    let mut value = deletion();
    value.created_at_ms = u64::MAX;
    invalid(repository.begin_object_deletion(&value));

    let (_temporary, repository) = open_repository();
    seed_complete_version(&repository);
    repository
        .begin_object_deletion(&deletion())
        .expect("deletion plan");
    invalid(repository.finish_deletion("delete_fixture", u64::MAX, &event("object.deleted")));
}

fn retention_policy_timestamps_fail_closed() {
    for (created_at_ms, updated_at_ms) in [(u64::MAX, u64::MAX), (1, u64::MAX)] {
        let (_temporary, repository) = open_repository();
        seed_project(&repository);
        let mut value = policy();
        value.created_at_ms = created_at_ms;
        value.updated_at_ms = updated_at_ms;
        invalid(repository.set_retention(&value, &event("retention.policy_set")));
    }
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    repository
        .set_retention(&policy(), &event("retention.policy_set"))
        .expect("retention policy");
    invalid(repository.clear_retention(
        "project_fixture",
        u64::MAX,
        &event("retention.policy_cleared"),
    ));
}

fn retention_run_timestamps_fail_closed() {
    let (_temporary, repository) = repository_with_retention();
    invalid(repository.begin_retention(
        "project_fixture",
        "run_fixture",
        "fixture",
        "request_fixture",
        u64::MAX,
    ));

    let (_temporary, repository) = repository_with_retention();
    repository
        .begin_retention(
            "project_fixture",
            "run_fixture",
            "fixture",
            "request_fixture",
            1,
        )
        .expect("retention plan");
    invalid(repository.finish_deletion("run_fixture", u64::MAX, &event("retention.enforced")));
}

fn execute(repository: &SqliteRepository, sql: &str) {
    let connection = repository.connection.lock().expect("connection");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .expect("ignore constraints");
    connection.execute_batch(sql).expect("corrupt fixture");
}

#[test]
fn malformed_identity_rows_are_unavailable() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(&repository, "UPDATE workspaces SET slug = ''");
    unavailable(repository.list_workspaces());

    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(&repository, "UPDATE projects SET slug = ''");
    unavailable(repository.list_projects("workspace_fixture"));
}

#[test]
fn malformed_audit_rows_are_unavailable() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO audit_events (id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms) VALUES ('event', 'workspace_fixture', 'actor', 'action', 'request', 'target', 'bad', 1)",
    );
    unavailable(repository.list_audit("workspace_fixture", None, 1));
}

#[test]
fn malformed_deletion_item_is_unavailable() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO deletion_operations (id, project_id, object_path, reason, status, actor, request_id, created_at_ms) VALUES ('delete_fixture', 'project_fixture', 'fixture.bin', 'object_delete', 'pending', 'fixture', 'request_fixture', 1); INSERT INTO deletion_items (operation_id, version_id, storage_key, version) VALUES ('delete_fixture', 'version_fixture', 'objects/version_fixture', -1)",
    );
    let mut value = deletion();
    value.target.version = None;
    unavailable(repository.begin_object_deletion(&value));
}

#[test]
fn malformed_retention_rows_are_unavailable() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO retention_policies (project_id, keep_latest, enabled, created_at_ms, updated_at_ms) VALUES ('project_fixture', -1, 1, 1, 1)",
    );
    unavailable(repository.retention_policy("project_fixture"));

    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO retention_runs (id, project_id, candidate_count, deleted_count, status, started_at_ms) VALUES ('run_fixture', 'project_fixture', -1, 0, 'running', 1)",
    );
    unavailable(repository.retention_overview("project_fixture"));
}

#[test]
fn malformed_transfer_rows_are_unavailable() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms) VALUES ('version_fixture', 'project_fixture', 'fixture.bin', -1, 'objects/version_fixture', 'complete', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 1); INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state, received_size, received_checksum) VALUES ('upload_fixture', 'version_fixture', 'fixture.bin', 'application/octet-stream', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 2, 'complete', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')",
    );
    unavailable(repository.list_stored_objects("project_fixture", None, false));
}

#[test]
fn upload_completion_rolls_back_when_the_version_cannot_transition() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    let value = upload();
    repository
        .reserve_upload(&value)
        .expect("upload reservation");
    repository
        .record_uploaded_bytes(&value.id, value.expected_size, &value.expected_checksum)
        .expect("uploaded bytes");
    execute(
        &repository,
        "UPDATE object_versions SET state = 'aborted' WHERE id = 'upload_fixture'",
    );

    assert_eq!(
        repository.complete_upload(&value.id),
        Err(RepositoryError::Conflict)
    );
    let persisted = repository
        .upload_by_id(&value.id)
        .expect("persisted upload");
    assert_eq!(
        persisted.state,
        blobyard_contract::ReservationState::Uploaded
    );
    assert_eq!(
        persisted.version.state,
        blobyard_contract::UploadState::Aborted
    );
}

#[test]
fn upload_abort_rolls_back_when_the_version_cannot_transition() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    let value = upload();
    repository
        .reserve_upload(&value)
        .expect("upload reservation");
    execute(
        &repository,
        "UPDATE object_versions SET state = 'aborted' WHERE id = 'upload_fixture'",
    );

    assert_eq!(
        repository.abort_upload(&value.id),
        Err(RepositoryError::Conflict)
    );
    let persisted = repository
        .upload_by_id(&value.id)
        .expect("persisted upload");
    assert_eq!(
        persisted.state,
        blobyard_contract::ReservationState::Requested
    );
    assert_eq!(
        persisted.version.state,
        blobyard_contract::UploadState::Aborted
    );
}

#[test]
fn upload_reservation_rejects_a_negative_persisted_version_without_mutation() {
    let (_temporary, repository) = open_repository();
    seed_project(&repository);
    execute(
        &repository,
        "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state) VALUES ('corrupt_version', 'project_fixture', 'fixture.bin', -2, 'objects/corrupt_version', 'pending')",
    );

    assert_eq!(
        repository.reserve_upload(&upload()),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        repository.upload_by_id("upload_fixture"),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn retention_resume_failure_preserves_the_failed_run() {
    let (_temporary, repository) = repository_with_retention();
    repository
        .begin_retention(
            "project_fixture",
            "run_fixture",
            "fixture",
            "request_fixture",
            2,
        )
        .expect("retention run");
    repository
        .fail_retention("run_fixture", 3)
        .expect("failed retention run");
    execute(
        &repository,
        "CREATE TRIGGER reject_retention_resume BEFORE UPDATE OF status ON retention_runs WHEN NEW.status = 'running' BEGIN SELECT RAISE(ABORT, 'reject resume'); END",
    );

    assert_eq!(
        repository.begin_retention(
            "project_fixture",
            "ignored_run",
            "ignored_actor",
            "ignored_request",
            4,
        ),
        Err(RepositoryError::Conflict)
    );
    let run = repository
        .retention_overview("project_fixture")
        .expect("retention overview")
        .last_run
        .expect("failed run");
    assert_eq!(run.status, "failed");
    assert_eq!(
        run.error_summary.as_deref(),
        Some("Storage deletion did not complete.")
    );
}
