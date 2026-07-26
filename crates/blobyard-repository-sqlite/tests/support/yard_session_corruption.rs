#![allow(
    clippy::redundant_pub_crate,
    reason = "path-based test support must expose fixtures to its parent module"
)]

use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;

const FIXTURE_SQL: &str = include_str!("yard_group_race.sql");

#[derive(Clone, Copy, Debug)]
pub(super) enum Corruption {
    ActiveGrantWithRevocation,
    ActiveGroupWithDeactivation,
    CrossWorkspaceGroup,
    InvalidMembershipTimestamp,
    IncorrectMemberCount,
    NonmatchingEnvironment,
    OverLimitActiveGroupGrants,
    OverLimitMembershipRows,
    SameNameForeignGroup,
    UnresolvedGroup,
}

pub(super) const CORRUPTIONS: [Corruption; 10] = [
    Corruption::ActiveGrantWithRevocation,
    Corruption::ActiveGroupWithDeactivation,
    Corruption::CrossWorkspaceGroup,
    Corruption::InvalidMembershipTimestamp,
    Corruption::IncorrectMemberCount,
    Corruption::NonmatchingEnvironment,
    Corruption::OverLimitActiveGroupGrants,
    Corruption::OverLimitMembershipRows,
    Corruption::SameNameForeignGroup,
    Corruption::UnresolvedGroup,
];

pub(super) struct Fixture {
    _temporary: tempfile::TempDir,
    pub(super) path: std::path::PathBuf,
    pub(super) repository: SqliteRepository,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join("metadata.sqlite3");
        let repository = SqliteRepository::open(&path).expect("repository");
        Connection::open(&path)
            .expect("fixture connection")
            .execute_batch(FIXTURE_SQL)
            .expect("fixture");
        Self {
            _temporary: temporary,
            path,
            repository,
        }
    }

    pub(super) fn set_corruption(&self, corruption: Corruption, corrupt: bool) {
        Connection::open(&self.path)
            .expect("corruption connection")
            .execute_batch(corruption_sql(corruption, corrupt))
            .expect("corruption state");
    }
}

const fn corruption_sql(corruption: Corruption, corrupt: bool) -> &'static str {
    match corruption {
        Corruption::ActiveGrantWithRevocation
        | Corruption::ActiveGroupWithDeactivation
        | Corruption::InvalidMembershipTimestamp
        | Corruption::IncorrectMemberCount => lifecycle_corruption_sql(corruption, corrupt),
        Corruption::CrossWorkspaceGroup
        | Corruption::SameNameForeignGroup
        | Corruption::UnresolvedGroup => identity_corruption_sql(corruption, corrupt),
        Corruption::NonmatchingEnvironment => environment_corruption_sql(corrupt),
        Corruption::OverLimitActiveGroupGrants | Corruption::OverLimitMembershipRows => {
            capacity_corruption_sql(corruption, corrupt)
        }
    }
}

const fn lifecycle_corruption_sql(corruption: Corruption, corrupt: bool) -> &'static str {
    match (corruption, corrupt) {
        (Corruption::ActiveGrantWithRevocation, true) => {
            "PRAGMA ignore_check_constraints = ON;
             UPDATE yard_access_grants SET revoked_at_ms = 3 WHERE id = 'grant_fixture';"
        }
        (Corruption::ActiveGrantWithRevocation, false) => {
            "UPDATE yard_access_grants SET revoked_at_ms = NULL WHERE id = 'grant_fixture';"
        }
        (Corruption::ActiveGroupWithDeactivation, true) => {
            "PRAGMA ignore_check_constraints = ON;
             UPDATE workspace_groups SET deactivated_at_ms = 3
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::ActiveGroupWithDeactivation, false) => {
            "UPDATE workspace_groups SET deactivated_at_ms = NULL
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::InvalidMembershipTimestamp, true) => {
            "PRAGMA ignore_check_constraints = ON;
             UPDATE workspace_group_members SET added_at_ms = -1
             WHERE group_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::InvalidMembershipTimestamp, false) => {
            "UPDATE workspace_group_members SET added_at_ms = 2
             WHERE group_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::IncorrectMemberCount, true) => {
            "UPDATE workspace_groups SET member_count = 2
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::IncorrectMemberCount, false) => {
            "UPDATE workspace_groups SET member_count = 1
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        _ => unreachable!(),
    }
}

const fn identity_corruption_sql(corruption: Corruption, corrupt: bool) -> &'static str {
    match (corruption, corrupt) {
        (Corruption::CrossWorkspaceGroup, true) => foreign_group_sql(false),
        (Corruption::CrossWorkspaceGroup | Corruption::SameNameForeignGroup, false) => {
            "UPDATE yard_access_grants
             SET principal_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
             WHERE id = 'grant_fixture';
             DELETE FROM workspace_group_members
             WHERE group_id = 'group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
             DELETE FROM workspace_groups
             WHERE id = 'group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
             DELETE FROM local_users WHERE id = 'user_foreign';
             DELETE FROM workspaces WHERE id = 'workspace_foreign';"
        }
        (Corruption::SameNameForeignGroup, true) => foreign_group_sql(true),
        (Corruption::UnresolvedGroup, true) => {
            "UPDATE yard_access_grants
             SET principal_id = 'group_ffffffffffffffffffffffffffffffff'
             WHERE id = 'grant_fixture';"
        }
        (Corruption::UnresolvedGroup, false) => {
            "UPDATE yard_access_grants
             SET principal_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
             WHERE id = 'grant_fixture';"
        }
        _ => unreachable!(),
    }
}

const fn environment_corruption_sql(corrupt: bool) -> &'static str {
    if corrupt {
        "INSERT INTO yard_environments
           (id, yard_id, name, kind, status, created_at_ms, updated_at_ms)
         VALUES
           ('environment_staging', 'yard_fixture', 'staging', 'staging', 'active', 2, 2);
         UPDATE yard_access_grants SET environment_id = 'environment_staging'
         WHERE id = 'grant_fixture';"
    } else {
        "UPDATE yard_access_grants SET environment_id = NULL WHERE id = 'grant_fixture';
         DELETE FROM yard_environments WHERE id = 'environment_staging';"
    }
}

const fn capacity_corruption_sql(corruption: Corruption, corrupt: bool) -> &'static str {
    match (corruption, corrupt) {
        (Corruption::OverLimitActiveGroupGrants, true) => {
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 500
             )
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             SELECT printf('grant_corrupt_%03d', value), 'yard_fixture', NULL, 'group',
                    'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '[]',
                    'active', 2, 'fixture', NULL, NULL
             FROM numbers;"
        }
        (Corruption::OverLimitActiveGroupGrants, false) => {
            "DELETE FROM yard_access_grants WHERE id LIKE 'grant_corrupt_%';"
        }
        (Corruption::OverLimitMembershipRows, true) => {
            "PRAGMA ignore_check_constraints = ON;
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 500
             )
             INSERT INTO local_users
               (id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms)
             SELECT printf('user_corrupt_%03d', value), 'workspace_fixture',
                    printf('Corrupt user %d', value), NULL, 'active', 2, NULL
             FROM numbers;
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 500
             )
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             SELECT 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_fixture',
                    printf('user_corrupt_%03d', value), 2
             FROM numbers;
             UPDATE workspace_groups SET member_count = 501
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        (Corruption::OverLimitMembershipRows, false) => {
            "DELETE FROM workspace_group_members WHERE user_id LIKE 'user_corrupt_%';
             DELETE FROM local_users WHERE id LIKE 'user_corrupt_%';
             UPDATE workspace_groups SET member_count = 1
             WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
        }
        _ => unreachable!(),
    }
}

const fn foreign_group_sql(same_name: bool) -> &'static str {
    if same_name {
        "INSERT INTO workspaces VALUES ('workspace_foreign', 'Foreign', 'foreign');
         INSERT INTO workspace_groups VALUES
           ('group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'workspace_foreign',
            'Readers', 'active', 0, 2, NULL);
         UPDATE yard_access_grants
         SET principal_id = 'group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
         WHERE id = 'grant_fixture';"
    } else {
        "INSERT INTO workspaces VALUES ('workspace_foreign', 'Foreign', 'foreign');
         INSERT INTO local_users VALUES
           ('user_foreign', 'workspace_foreign', 'Foreign user', NULL, 'active', 2, NULL);
         INSERT INTO workspace_groups VALUES
           ('group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'workspace_foreign',
            'Foreign group', 'active', 1, 2, NULL);
         INSERT INTO workspace_group_members VALUES
           ('group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'workspace_foreign',
            'user_foreign', 2);
         UPDATE yard_access_grants
         SET principal_id = 'group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
         WHERE id = 'grant_fixture';"
    }
}
