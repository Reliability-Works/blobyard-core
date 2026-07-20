use super::*;

#[path = "adapter_failure_map_tests/yards.rs"]
mod poisoned_yards;

fn poison(repository: &SqliteRepository) {
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = repository.test_connection().expect("connection");
        std::panic::resume_unwind(Box::new(()));
    }));
    assert!(unwind.is_err());
}

fn interrupt_queries(repository: &SqliteRepository) {
    let connection = repository.connection.lock().expect("connection");
    connection
        .progress_handler(1, Some(|| true))
        .expect("progress handler");
}

fn assert_poisoned_metadata(repository: &SqliteRepository) {
    unavailable(repository.schema_version());
    unavailable(repository.create_workspace(&workspace()));
    unavailable(repository.list_workspaces());
    unavailable(repository.workspace_by_slug(&slug("fixture")));
    unavailable(repository.create_project(&project()));
    unavailable(repository.list_projects("workspace_fixture"));
    unavailable(repository.project_by_slug("workspace_fixture", &slug("project")));
    unavailable(repository.reserve_object_version(&version()));
    unavailable(repository.complete_object_version("version_fixture", 1, &checksum('a')));
    unavailable(repository.abort_object_version("version_fixture"));
    unavailable(repository.object_version("version_fixture"));
}

fn assert_poisoned_credentials(repository: &SqliteRepository) {
    let token = token();
    let create_event = token_audit("api_token.created", &token.id, token.created_at_ms);
    let revoke_event = token_audit("api_token.revoked", &token.id, 2);
    unavailable(repository.install_bootstrap(&checksum('b')));
    unavailable(repository.exchange_bootstrap(&checksum('b'), &token, &session(&token)));
    unavailable(repository.list_cli_sessions("workspace_fixture"));
    unavailable(repository.create_api_token(&token, &create_event));
    unavailable(repository.list_api_tokens());
    unavailable(repository.authenticate_api_token(&checksum('c'), 2));
    unavailable(repository.revoke_api_token("token_fixture", 2, &revoke_event));
    unavailable(repository.revoke_cli_session(
        "session_fixture",
        "workspace_fixture",
        2,
        &revoke_event,
    ));
}

fn assert_poisoned_ci(repository: &SqliteRepository) {
    let trust = ci_fixtures::trust("trust_fixture", None, 1);
    let session = ci_fixtures::session(1, 10);
    unavailable(repository.create_ci_trust(
        &trust,
        &ci_fixtures::event("ci.trust_created", "ci_trust", &trust.id, 1),
    ));
    unavailable(repository.list_ci_trusts(&trust.workspace_id));
    unavailable(repository.mint_machine_session(
        &session,
        &ci_fixtures::event("ci.token_minted", "project", "project_fixture", 10),
    ));
    unavailable(repository.authenticate_machine_session(&session.id, 11));
    unavailable(repository.revoke_ci_trust(
        &trust.id,
        &trust.workspace_id,
        20,
        &ci_fixtures::event("ci.trust_revoked", "ci_trust", &trust.id, 20),
    ));
}

fn assert_poisoned_transfers(repository: &SqliteRepository) {
    let part = blobyard_contract::NewUploadPartGrant {
        upload_id: "upload_fixture".to_owned(),
        part_number: 1,
        expected_size: 1,
        capability_hash: checksum('e'),
        expires_at_ms: 2,
    };
    unavailable(repository.reserve_upload(&upload()));
    unavailable(repository.upload_by_capability(&checksum('b'), 1));
    unavailable(repository.upload_by_id("upload_fixture"));
    unavailable(repository.renew_upload("upload_fixture", 2));
    unavailable(repository.attach_multipart("upload_fixture", "provider_fixture"));
    unavailable(repository.issue_upload_parts(std::slice::from_ref(&part)));
    unavailable(repository.upload_part_by_capability(&part.capability_hash, 1));
    unavailable(repository.record_uploaded_part("upload_fixture", 1, 1, &checksum('a'), None));
    unavailable(repository.list_upload_parts("upload_fixture"));
    unavailable(repository.record_uploaded_bytes("upload_fixture", 1, &checksum('a')));
    unavailable(repository.complete_upload("upload_fixture"));
    unavailable(repository.abort_upload("upload_fixture"));
    unavailable(repository.list_stored_objects("project_fixture", None, false));
    unavailable(repository.issue_download(&NewDownloadGrant {
        version_id: "version_fixture".to_owned(),
        capability_hash: checksum('d'),
        expires_at_ms: 2,
    }));
    unavailable(repository.download_by_capability(&checksum('d'), 1));
}

fn assert_poisoned_sharing(repository: &SqliteRepository) {
    let share = NewShare {
        id: "share_failure_map".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        version_id: "upload_two".to_owned(),
        capability_hash: checksum('e'),
        expires_at_ms: 5_000,
        maximum_downloads: Some(2),
        created_at_ms: 1_000,
    };
    let event = |action: &str, created_at_ms| NewAuditEvent {
        id: format!("audit_share_failure_{created_at_ms}"),
        workspace_id: share.workspace_id.clone(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_share_failure_{created_at_ms}"),
        target_type: "share".to_owned(),
        metadata: vec![(
            "shareId".to_owned(),
            blobyard_contract::AuditValue::String(share.id.clone()),
        )],
        created_at_ms,
    };
    unavailable(repository.create_share(&share, &event("share.created", 1_000)));
    unavailable(repository.list_shares(&share.workspace_id));
    unavailable(repository.share_by_capability(&share.capability_hash, 1_001));
    unavailable(repository.issue_share_download(
        &share.capability_hash,
        1_001,
        &NewDownloadGrant {
            version_id: share.version_id.clone(),
            capability_hash: checksum('f'),
            expires_at_ms: 1_100,
        },
        &event("share.download_issued", 1_001),
    ));
    unavailable(repository.revoke_share(
        &share.id,
        &share.workspace_id,
        1_002,
        &event("share.revoked", 1_002),
    ));
}

fn assert_poisoned_inboxes(repository: &SqliteRepository) {
    let inbox = NewInbox {
        id: "inbox_failure_map".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: "Failure inbox".to_owned(),
        capability_hash: checksum('f'),
        expires_at_ms: 5_000,
        maximum_files: 2,
        maximum_bytes: 10,
        created_at_ms: 1_000,
    };
    let upload = upload();
    let principal = NewInboxUpload {
        capability_hash: inbox.capability_hash.clone(),
        fingerprint_hash: checksum('0'),
        now_ms: upload.created_at_ms,
    };
    unavailable(repository.create_inbox(
        &inbox,
        &blobyard_testkit::inbox_event("inbox.created", &inbox.id, inbox.created_at_ms),
    ));
    unavailable(repository.list_inboxes(&inbox.project_id));
    unavailable(repository.inbox_by_capability(&inbox.capability_hash, 1_001));
    unavailable(repository.consume_inbox_rate(&checksum('1'), 1_000, 2, 1_000));
    unavailable(repository.reserve_inbox_upload(&principal, &upload));
    unavailable(repository.inbox_upload_by_id(&inbox.capability_hash, &upload.id, 1_001));
    unavailable(repository.complete_inbox_upload(
        &inbox.capability_hash,
        &upload.id,
        1_001,
        &blobyard_testkit::inbox_upload_event(&inbox.id, 1_001),
    ));
    unavailable(repository.abort_inbox_upload(&inbox.capability_hash, &upload.id, 1_001));
    unavailable(repository.revoke_inbox(
        &inbox.id,
        &inbox.workspace_id,
        1_001,
        &blobyard_testkit::inbox_event("inbox.revoked", &inbox.id, 1_001),
    ));
}

fn assert_poisoned_previews(repository: &SqliteRepository) {
    let preview = NewPreview {
        id: "preview_failure_map".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        capability_hash: checksum('9'),
        expires_at_ms: 5_000,
        created_at_ms: 1_000,
        files: vec![NewPreviewFile {
            normalized_path: "index.html".to_owned(),
            version_id: "upload_two".to_owned(),
        }],
    };
    unavailable(repository.create_preview(
        &preview,
        &blobyard_testkit::preview_event("preview.created", &preview.id, 1_000),
    ));
    unavailable(repository.list_previews(&preview.project_id));
    unavailable(repository.preview_by_id(&preview.id));
    unavailable(repository.preview_file_by_capability(
        &preview.capability_hash,
        "index.html",
        1_001,
    ));
    unavailable(repository.revoke_preview(
        &preview.id,
        &preview.workspace_id,
        &preview.project_id,
        1_001,
        &blobyard_testkit::preview_event("preview.revoked", &preview.id, 1_001),
    ));
}

fn assert_poisoned_lifecycle(repository: &SqliteRepository) {
    let policy = policy();
    unavailable(repository.record_audit(&audit()));
    unavailable(repository.list_audit("workspace_fixture", None, 1));
    unavailable(repository.begin_object_deletion(&deletion()));
    unavailable(repository.finish_deletion("delete_fixture", 2, &audit()));
    unavailable(repository.retention_policy("project_fixture"));
    unavailable(repository.set_retention(&policy, &audit()));
    unavailable(repository.clear_retention("project_fixture", 2, &audit()));
    unavailable(repository.retention_overview("project_fixture"));
    unavailable(repository.begin_retention(
        "project_fixture",
        "run_fixture",
        "fixture",
        "request_fixture",
        1,
    ));
    unavailable(repository.fail_retention("run_fixture", 2));
    unavailable(repository.retained_projects());
}

fn assert_every_public_operation(repository: &SqliteRepository) {
    assert_poisoned_metadata(repository);
    assert_poisoned_credentials(repository);
    assert_poisoned_ci(repository);
    assert_poisoned_transfers(repository);
    assert_poisoned_sharing(repository);
    assert_poisoned_inboxes(repository);
    assert_poisoned_previews(repository);
    poisoned_yards::assert_poisoned_yards(repository);
    assert_poisoned_lifecycle(repository);
}

#[test]
fn sqlite_maps_every_statement_denial_to_the_stable_unavailable_error() {
    assert!(assert_denial_sweep(denied_initialization).expect("bounded initialization sweep") > 0);
    assert!(assert_denial_sweep(denied_contract).expect("bounded contract sweep") > 0);
}

#[test]
fn sqlite_maps_a_poisoned_connection_lock_on_every_public_operation() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    poison(&repository);
    assert_every_public_operation(&repository);
}

#[test]
fn sqlite_maps_interrupted_statements_on_every_public_operation() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    run_contract(&repository).expect("seed repository");
    interrupt_queries(&repository);
    assert_every_public_operation(&repository);
}
