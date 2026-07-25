#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::admission;
use blobyard_contract::{
    LifecycleRepository, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use rusqlite::Connection;

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: super::super::SqliteRepository,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let repository =
            super::super::SqliteRepository::open(&temporary.path().join("metadata.sqlite3"))
                .expect("repository");
        repository
            .test_connection()
            .expect("connection")
            .execute_batch(
                "INSERT INTO workspaces VALUES
                   ('workspace_owner', 'Owner', 'owner'),
                   ('workspace_foreign', 'Foreign', 'foreign');
                 INSERT INTO projects VALUES
                   ('project_owner', 'workspace_owner', 'Owner project', 'owner-project');
                 INSERT INTO object_versions
                   (id, project_id, object_path, version, storage_key, state, size, checksum)
                 VALUES
                   ('version_owner', 'project_owner', 'asset.js', 1, 'objects/version_owner',
                    'complete', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
                 INSERT INTO upload_reservations
                   (id, version_id, filename, content_type, expected_size, expected_checksum,
                    capability_hash, expires_at_ms, state, received_size, received_checksum)
                 VALUES
                   ('upload_owner', 'version_owner', 'asset.js', 'text/javascript', 1,
                    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                    1000000, 'complete', 1,
                    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
                 INSERT INTO local_users VALUES
                   ('user_foreign', 'workspace_foreign', 'Foreign user', NULL, 'active', 1, NULL);
                 INSERT INTO web_yards VALUES
                   ('yard_owner', 'workspace_owner', 'project_owner', 'docs', 'docs-fixture',
                    'deploy_owner', 'active', 1, 1, NULL);
                 INSERT INTO yard_deploys VALUES
                   ('deploy_owner', 'yard_owner', 'workspace_owner', 'project_owner',
                    'clientdeploy00000001', '.blobyard-yard/yard_owner/clientdeploy00000001/',
                    'docs-deploy-fixture', 0, 0, 'live', 1, 2, 1, 1, NULL, NULL, NULL);
                 INSERT INTO yard_deploy_files VALUES
                   ('deploy_owner', 'asset.js', 'version_owner', 1);
                 INSERT INTO yard_environments VALUES
                   ('environment_owner', 'yard_owner', 'production', 'production', 'active',
                    1, 1, NULL);
                 INSERT INTO yard_access_policies VALUES
                   ('yard_owner', 'any-authenticated', 1, 'fixture');",
            )
            .expect("fixture");
        Self {
            _temporary: temporary,
            repository,
        }
    }

    fn set_selected_with_cross_tenant_grant(&self) {
        self.repository
            .test_connection()
            .expect("connection")
            .execute_batch(
                "UPDATE yard_access_policies SET visibility = 'selected';
                 INSERT OR IGNORE INTO yard_access_grants VALUES
                   ('grant_cross_tenant', 'yard_owner', NULL, 'user', 'user_foreign', '[]',
                    'active', 2, 'fixture', NULL, NULL);",
            )
            .expect("cross-tenant grant");
    }

    fn set_any_authenticated(&self) {
        self.repository
            .test_connection()
            .expect("connection")
            .execute(
                "UPDATE yard_access_policies SET visibility = 'any-authenticated'",
                [],
            )
            .expect("visibility");
    }

    fn set_selected_with_corrupt_group_membership(&self) {
        self.repository
            .test_connection()
            .expect("connection")
            .execute_batch(
                "PRAGMA foreign_keys = OFF;
                 UPDATE yard_access_policies SET visibility = 'selected';
                 INSERT INTO workspace_groups
                   (id, workspace_id, name, status, member_count, created_at_ms)
                 VALUES
                   ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_owner',
                    'Corrupt membership', 'active', 1, 2);
                 INSERT INTO workspace_group_members VALUES
                   ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_owner',
                    'user_foreign', 2);
                 INSERT INTO yard_access_grants VALUES
                   ('grant_corrupt_membership', 'yard_owner', NULL, 'group',
                    'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '[]',
                    'active', 2, 'fixture', NULL, NULL);
                 PRAGMA foreign_keys = ON;",
            )
            .expect("corrupt membership");
    }
}

fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn continuation(id: &str, code_character: char, at_ms: u64) -> NewYardContinuation {
    NewYardContinuation {
        id: id.to_owned(),
        continuation_hash: hash('c'),
        code_hash: hash(code_character),
        yard_id: "yard_owner".to_owned(),
        environment_id: "environment_owner".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_foreign".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: at_ms,
        expires_at_ms: at_ms + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn session(id: &str, token_character: char, at_ms: u64) -> NewYardSession {
    NewYardSession {
        id: id.to_owned(),
        token_hash: hash(token_character),
        created_at_ms: at_ms,
        expires_at_ms: at_ms + YARD_SESSION_LIFETIME_MS,
    }
}

fn audit(id: &str) -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: id.to_owned(),
        request_id: format!("request_{id}"),
    }
}

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

#[test]
fn issue_rejects_a_direct_user_grant_from_another_workspace() {
    let fixture = Fixture::new();
    fixture.set_selected_with_cross_tenant_grant();
    assert_eq!(
        fixture
            .repository
            .issue_yard_exchange_code(&continuation("continuation_issue", '1', 10)),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn issue_rejects_a_corrupt_cross_tenant_group_membership() {
    let fixture = Fixture::new();
    fixture.set_selected_with_corrupt_group_membership();
    assert_eq!(
        fixture.repository.issue_yard_exchange_code(&continuation(
            "continuation_membership",
            '6',
            10
        )),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn exchange_rechecks_tenant_admission_and_rolls_back_code_consumption() {
    let fixture = Fixture::new();
    let continuation = continuation("continuation_exchange", '2', 20);
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue under any-authenticated");
    fixture.set_selected_with_cross_tenant_grant();
    let session = session("session_exchange", '4', 21);
    let audit = audit("audit_exchange_tenant");
    assert_eq!(
        fixture.repository.exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit,
            21,
        ),
        Err(RepositoryError::NotFound)
    );
    fixture.set_any_authenticated();
    assert!(
        fixture
            .repository
            .exchange_yard_session_code(
                &continuation.code_hash,
                &continuation.host_label,
                &session,
                &audit,
                21,
            )
            .is_ok()
    );
    let audits = fixture
        .repository
        .list_audit("workspace_owner", None, 50)
        .expect("audit");
    assert_eq!(
        audits
            .items
            .iter()
            .filter(|event| event.id == audit.id)
            .count(),
        1
    );
}

#[test]
fn delivery_rejects_a_direct_user_grant_from_another_workspace() {
    let fixture = Fixture::new();
    let continuation = continuation("continuation_delivery", '3', 30);
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let session = session("session_delivery", '5', 31);
    fixture
        .repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit("audit_delivery_tenant"),
            31,
        )
        .expect("exchange");
    fixture.set_selected_with_cross_tenant_grant();
    assert_eq!(
        fixture.repository.yard_file_by_host(
            &continuation.host_label,
            "asset.js",
            Some(&session.token_hash),
            32,
        ),
        Err(RepositoryError::NotFound)
    );
    fixture.set_any_authenticated();
    assert!(
        fixture
            .repository
            .yard_file_by_host(
                &continuation.host_label,
                "asset.js",
                Some(&session.token_hash),
                32,
            )
            .is_ok()
    );
}
