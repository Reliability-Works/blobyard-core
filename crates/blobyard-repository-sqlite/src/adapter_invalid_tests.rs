use super::*;
use blobyard_contract::{AuditValue, LocalApiTokenRecord, LocalCliSessionRecord};

#[test]
fn public_repository_inputs_fail_closed_at_each_field_boundary() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    assert_invalid_metadata(&repository);
    assert_invalid_credentials(&repository);
    assert_invalid_transfers(&repository);
    assert_invalid_lifecycle(&repository);
}

fn assert_invalid_metadata(repository: &SqliteRepository) {
    let mut invalid_workspace = workspace();
    invalid_workspace.id.clear();
    invalid(repository.create_workspace(&invalid_workspace));
    invalid_workspace = workspace();
    invalid_workspace.name.clear();
    invalid(repository.create_workspace(&invalid_workspace));
    invalid(repository.rename_workspace(&invalid_workspace, &audit()));
    let mut invalid_project = project();
    invalid_project.id.clear();
    invalid(repository.create_project(&invalid_project));
    invalid_project = project();
    invalid_project.name.clear();
    invalid(repository.create_project(&invalid_project));
    invalid_project = project();
    invalid_project.workspace_id.clear();
    invalid(repository.create_project(&invalid_project));
    invalid(repository.list_projects(""));
    invalid(repository.project_by_slug("", &slug("project")));
    for invalid_version in invalid_versions() {
        invalid(repository.reserve_object_version(&invalid_version));
    }
    invalid(repository.complete_object_version("", 1, &checksum('a')));
    invalid(repository.complete_object_version("version_fixture", 1, "bad"));
    invalid(repository.complete_object_version("version_fixture", u64::MAX, &checksum('a')));
    invalid(repository.abort_object_version(""));
    invalid(repository.object_version(""));
}

fn assert_invalid_credentials(repository: &SqliteRepository) {
    let event = audit();
    invalid(repository.install_bootstrap("bad"));
    invalid(repository.authenticate_api_token("bad", 1));
    invalid(repository.authenticate_api_token(&checksum('a'), u64::MAX));
    invalid(repository.revoke_api_token("", 1, &event));
    invalid(repository.revoke_api_token("token", u64::MAX, &event));
    invalid(repository.list_cli_sessions(""));
    invalid(repository.revoke_cli_session("", "workspace", 1, &event));
    invalid(repository.revoke_cli_session("session", "", 1, &event));
    invalid(repository.revoke_cli_session("session", "workspace", u64::MAX, &event));
    for invalid_token in invalid_tokens() {
        invalid(repository.exchange_bootstrap(
            &checksum('b'),
            &invalid_token,
            &session(&invalid_token),
        ));
        invalid(repository.create_api_token(&invalid_token, &event));
    }
    let token = token();
    for invalid_session in invalid_sessions(&token) {
        invalid(repository.exchange_bootstrap(&checksum('b'), &token, &invalid_session));
    }
    invalid(repository.exchange_bootstrap("bad", &token, &session(&token)));
}

fn assert_invalid_transfers(repository: &SqliteRepository) {
    for invalid_upload in invalid_uploads() {
        invalid(repository.reserve_upload(&invalid_upload));
    }
    invalid(repository.upload_by_capability("bad", 1));
    invalid(repository.upload_by_capability(&checksum('b'), u64::MAX));
    invalid(repository.upload_by_id(""));
    invalid(repository.renew_upload("", 1));
    invalid(repository.renew_upload("upload_fixture", u64::MAX));
    invalid(repository.record_uploaded_bytes("", 1, &checksum('a')));
    invalid(repository.record_uploaded_bytes("upload_fixture", 1, "bad"));
    invalid(repository.record_uploaded_bytes("upload_fixture", u64::MAX, &checksum('a')));
    invalid(repository.complete_upload(""));
    invalid(repository.abort_upload(""));
    invalid(repository.list_stored_objects("", None, false));
    invalid(repository.list_stored_objects("project_fixture", Some(""), false));
    for invalid_download in invalid_downloads() {
        invalid(repository.issue_download(&invalid_download));
    }
    invalid(repository.download_by_capability("bad", 1));
    invalid(repository.download_by_capability(&checksum('d'), u64::MAX));
}

fn assert_invalid_lifecycle(repository: &SqliteRepository) {
    for invalid_audit in invalid_audits() {
        invalid(repository.record_audit(&invalid_audit));
    }
    invalid(repository.list_audit("", None, 1));
    invalid(repository.list_audit("workspace_fixture", Some(u64::MAX), 1));
    invalid(repository.list_audit("workspace_fixture", None, 0));
    invalid(repository.list_audit("workspace_fixture", None, 101));
    for invalid_deletion in invalid_deletions() {
        invalid(repository.begin_object_deletion(&invalid_deletion));
    }
    invalid(repository.retention_policy(""));
    invalid(repository.retention_overview(""));
    for invalid_policy in invalid_policies() {
        invalid(repository.set_retention(&invalid_policy, &audit()));
    }
    invalid(repository.clear_retention("", 1, &audit()));
    for arguments in [
        ("", "run", "actor", "request"),
        ("project", "", "actor", "request"),
        ("project", "run", "", "request"),
        ("project", "run", "actor", ""),
    ] {
        invalid(repository.begin_retention(arguments.0, arguments.1, arguments.2, arguments.3, 1));
    }
    invalid(repository.fail_retention("", 1));
    invalid(repository.fail_retention("run_fixture", u64::MAX));
}

fn invalid_versions() -> Vec<NewObjectVersion> {
    let mut values = Vec::new();
    for field in 0..8 {
        let mut value = version();
        match field {
            0 => value.id.clear(),
            1 => value.project_id.clear(),
            2 => value.object_path.clear(),
            3 => value.version = 0,
            4 => value.storage_key = "../escape".to_owned(),
            5 => value.git_repository = Some(String::new()),
            6 => value.git_commit = Some(String::new()),
            _ => value.git_branch = Some(String::new()),
        }
        values.push(value);
    }
    values
}

fn invalid_sessions(token: &LocalApiTokenRecord) -> Vec<LocalCliSessionRecord> {
    let mut values = Vec::new();
    for field in 0..11 {
        let mut value = session(token);
        match field {
            0 => value.id.clear(),
            1 => value.token_id.clear(),
            2 => value.workspace_id.clear(),
            3 => value.name.clear(),
            4 => value.platform.clear(),
            5 => value.version.clear(),
            6 => value.token_id.push_str("_wrong"),
            7 => value.workspace_id.push_str("_wrong"),
            8 => value.name.push_str(" wrong"),
            9 => value.last_used_at_ms = Some(value.created_at_ms),
            _ => value.revoked_at_ms = Some(value.created_at_ms),
        }
        values.push(value);
    }
    let mut timestamp = session(token);
    timestamp.created_at_ms += 1;
    values.push(timestamp);
    values
}

fn invalid_tokens() -> Vec<LocalApiTokenRecord> {
    let mut values = Vec::new();
    for field in 0..11 {
        let mut value = token();
        match field {
            0 => value.id.clear(),
            1 => value.name.clear(),
            2 => value.secret_hash = "bad".to_owned(),
            3 => value.workspace_id.clear(),
            4 => value.scopes.clear(),
            5 => value.scopes = vec![String::new()],
            6 => value.token_prefix.clear(),
            7 => value.project_id = Some(String::new()),
            8 => value.expires_at_ms = value.created_at_ms,
            9 => value.last_used_at_ms = Some(0),
            _ => value.revoked_at_ms = Some(0),
        }
        values.push(value);
    }
    values
}

fn invalid_uploads() -> Vec<NewUploadReservation> {
    let mut values = Vec::new();
    for field in 0..14 {
        let mut value = upload();
        match field {
            0 => value.id.clear(),
            1 => value.project_id.clear(),
            2 => value.object_path.clear(),
            3 => value.filename.clear(),
            4 => value.content_type.clear(),
            5 => value.expected_checksum = "bad".to_owned(),
            6 => value.capability_hash = "bad".to_owned(),
            7 => value.storage_key = "../escape".to_owned(),
            8 => value.expected_size = u64::MAX,
            9 => value.expires_at_ms = u64::MAX,
            10 => value.created_at_ms = u64::MAX,
            11 => value.git_repository = Some(String::new()),
            12 => value.git_commit = Some(String::new()),
            _ => value.git_branch = Some(String::new()),
        }
        values.push(value);
    }
    values
}

fn invalid_downloads() -> [NewDownloadGrant; 3] {
    [
        NewDownloadGrant {
            version_id: String::new(),
            capability_hash: checksum('d'),
            expires_at_ms: 1,
        },
        NewDownloadGrant {
            version_id: "version_fixture".to_owned(),
            capability_hash: "bad".to_owned(),
            expires_at_ms: 1,
        },
        NewDownloadGrant {
            version_id: "version_fixture".to_owned(),
            capability_hash: checksum('d'),
            expires_at_ms: u64::MAX,
        },
    ]
}

fn invalid_audits() -> Vec<NewAuditEvent> {
    let mut values = Vec::new();
    for field in 0..10 {
        let mut value = audit();
        match field {
            0 => value.id.clear(),
            1 => value.workspace_id.clear(),
            2 => value.actor.clear(),
            3 => value.action.clear(),
            4 => value.request_id.clear(),
            5 => value.target_type.clear(),
            6 => value.metadata = vec![(String::new(), AuditValue::Null)],
            7 => value.metadata = vec![("name".to_owned(), AuditValue::String(String::new()))],
            8 => {
                value.metadata = vec![
                    ("name".to_owned(), AuditValue::Null),
                    ("name".to_owned(), AuditValue::Null),
                ];
            }
            _ => value.created_at_ms = u64::MAX,
        }
        values.push(value);
    }
    values
}

fn invalid_deletions() -> Vec<NewObjectDeletion> {
    let mut values = Vec::new();
    for field in 0..8 {
        let mut value = deletion();
        match field {
            0 => value.id.clear(),
            1 => value.target.project_id.clear(),
            2 => value.target.object_path.clear(),
            3 => value.actor.clear(),
            4 => value.request_id.clear(),
            5 => value.target.version = Some(0),
            6 => value.target.version = Some(u64::MAX),
            _ => value.created_at_ms = u64::MAX,
        }
        values.push(value);
    }
    values
}

fn invalid_policies() -> Vec<RetentionPolicyRecord> {
    let mut values = Vec::new();
    for field in 0..10 {
        let mut value = policy();
        match field {
            0 => value.project_id.clear(),
            1 => value.keep_latest = 0,
            2 => value.updated_at_ms = 0,
            3 => value.path_glob = Some(String::new()),
            4 => value.path_glob = Some(" /bad".to_owned()),
            5 => value.path_glob = Some("bad\\glob".to_owned()),
            6 => value.path_glob = Some("/absolute".to_owned()),
            7 => value.path_glob = Some("one/../two".to_owned()),
            8 => value.branch_glob = Some("bad\nbranch".to_owned()),
            _ => value.path_glob = Some("x".repeat(257)),
        }
        values.push(value);
    }
    values
}
