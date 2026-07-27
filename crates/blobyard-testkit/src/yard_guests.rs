use blobyard_contract::{NewYardContinuation, YARD_EXCHANGE_CODE_LIFETIME_MS};

/// Seeds the canonical `SQLite` guest-Yard scope used by repository tests.
pub const SQLITE_GUEST_YARD_SEED: &str =
    "INSERT INTO workspaces VALUES ('workspace_guest', 'Guest', 'guest');
     INSERT INTO projects VALUES
       ('project_guest', 'workspace_guest', 'Guest project', 'guest-project');
     INSERT INTO web_yards
       (id, workspace_id, project_id, name, host_label, status, created_at_ms, updated_at_ms)
     VALUES
       ('yard_guest', 'workspace_guest', 'project_guest', 'guest',
        'guest-yard-fixture', 'active', 1, 1);
     INSERT INTO yard_environments
       (id, yard_id, name, kind, status, created_at_ms, updated_at_ms)
     VALUES
       ('environment_guest', 'yard_guest', 'production', 'production', 'active', 1, 1);
     INSERT INTO yard_access_policies
       (yard_id, visibility, updated_at_ms, updated_by_principal)
     VALUES ('yard_guest', 'selected', 1, 'operator');";

/// Builds the canonical continuation for the `SQLite` guest-Yard fixture.
#[must_use]
pub fn sqlite_guest_yard_continuation() -> NewYardContinuation {
    NewYardContinuation {
        id: "continuation_guest".to_owned(),
        continuation_hash: crate::hash('e'),
        code_hash: crate::hash('f'),
        yard_id: "yard_guest".to_owned(),
        environment_id: "environment_guest".to_owned(),
        host_label: "guest-yard-fixture".to_owned(),
        user_id: "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: 2,
        expires_at_ms: 2 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}
