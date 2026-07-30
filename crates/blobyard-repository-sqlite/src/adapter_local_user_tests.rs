#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{checksum, invalid, stable_behavior::repository, token_behavior::mismatched_events};
use blobyard_contract::{
    LifecycleRepository, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository,
    LocalUserStatus, RepositoryError,
};
use blobyard_testkit::{local_user, local_user_event, login_key};

fn fixture_user() -> LocalUserRecord {
    local_user("workspace_fixture", "user_fixture", None, 1)
}

fn fixture_key() -> LocalUserLoginKeyRecord {
    login_key("userkey_fixture", "user_fixture", 'a', 1)
}

#[test]
fn local_user_mutations_and_audit_events_commit_atomically() {
    let (_temporary, repository) = repository();
    let user = fixture_user();
    let key = fixture_key();
    let create_collision = local_user_event("audit_user_created", &user, "user.created", 1);
    repository
        .record_audit(&create_collision)
        .expect("create audit collision");
    assert_eq!(
        repository.create_local_user(&user, &key, &create_collision),
        Err(RepositoryError::Conflict)
    );
    assert!(
        repository
            .list_local_users("workspace_fixture")
            .expect("users")
            .is_empty()
    );

    let mut create_event = create_collision;
    create_event.id = "audit_user_created_fresh".to_owned();
    repository
        .create_local_user(&user, &key, &create_event)
        .expect("user and create audit");
    let reset_collision =
        local_user_event("audit_user_created_fresh", &user, "user.login_key_reset", 2);
    let replacement = login_key("userkey_replacement", "user_fixture", 'b', 2);
    assert_eq!(
        repository.reset_local_user_login_key(&replacement, 2, &reset_collision),
        Err(RepositoryError::Conflict)
    );
    assert!(
        repository
            .authenticate_local_user_key(&key.secret_hash, 2)
            .is_ok(),
        "a failed reset must not revoke the active key"
    );

    let reset_event = local_user_event("audit_key_reset", &user, "user.login_key_reset", 2);
    repository
        .reset_local_user_login_key(&replacement, 2, &reset_event)
        .expect("replacement key and reset audit");
    let deactivate_event = local_user_event("audit_user_deactivated", &user, "user.deactivated", 3);
    repository
        .deactivate_local_user(&user.id, 3, &deactivate_event)
        .expect("deactivation and audit");
    let audit = repository
        .list_audit("workspace_fixture", None, 10)
        .expect("audit");
    let actions = audit
        .items
        .iter()
        .map(|event| event.action.as_str())
        .collect::<Vec<_>>();
    for expected in ["user.created", "user.login_key_reset", "user.deactivated"] {
        assert!(actions.contains(&expected), "missing {expected}");
    }
}

#[test]
fn local_user_mutations_reject_every_mismatched_audit_field() {
    let (_temporary, repository) = repository();
    let user = fixture_user();
    let key = fixture_key();
    let create_event = local_user_event("audit_user_created", &user, "user.created", 1);
    for event in mismatched_events(&create_event) {
        assert_eq!(
            repository.create_local_user(&user, &key, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    repository
        .create_local_user(&user, &key, &create_event)
        .expect("valid user");

    let replacement = login_key("userkey_replacement", "user_fixture", 'b', 2);
    let reset_event = local_user_event("audit_key_reset", &user, "user.login_key_reset", 2);
    for event in mismatched_events(&reset_event) {
        assert_eq!(
            repository.reset_local_user_login_key(&replacement, 2, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    let deactivate_event = local_user_event("audit_user_deactivated", &user, "user.deactivated", 2);
    for event in mismatched_events(&deactivate_event) {
        assert_eq!(
            repository.deactivate_local_user(&user.id, 2, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert!(
        repository
            .authenticate_local_user_key(&key.secret_hash, 2)
            .is_ok(),
        "rejected mutations must leave the user active"
    );
}

#[test]
fn local_user_persistence_rejects_each_timestamp_beyond_sqlite_range() {
    let (_temporary, repository) = repository();
    let mut user = fixture_user();
    let mut key = fixture_key();
    user.created_at_ms = i64::MAX as u64 + 1;
    key.created_at_ms = user.created_at_ms;
    key.expires_at_ms = u64::MAX;
    let event = local_user_event(
        "audit_user_overflow",
        &user,
        "user.created",
        user.created_at_ms,
    );
    assert_eq!(
        repository.create_local_user(&user, &key, &event),
        Err(RepositoryError::InvalidInput)
    );

    let user = fixture_user();
    let mut key = fixture_key();
    key.expires_at_ms = i64::MAX as u64 + 1;
    let event = local_user_event("audit_key_overflow", &user, "user.created", 1);
    assert_eq!(
        repository.create_local_user(&user, &key, &event),
        Err(RepositoryError::InvalidInput)
    );

    let mut replacement = login_key("userkey_overflow", "user_fixture", 'b', i64::MAX as u64 + 1);
    replacement.expires_at_ms = u64::MAX;
    let reset = local_user_event(
        "audit_reset_overflow",
        &user,
        "user.login_key_reset",
        replacement.created_at_ms,
    );
    assert_eq!(
        repository.reset_local_user_login_key(&replacement, replacement.created_at_ms, &reset),
        Err(RepositoryError::InvalidInput)
    );
    let deactivate = local_user_event(
        "audit_deactivate_overflow",
        &user,
        "user.deactivated",
        u64::MAX,
    );
    assert_eq!(
        repository.deactivate_local_user(&user.id, u64::MAX, &deactivate),
        Err(RepositoryError::InvalidInput)
    );
    assert!(
        repository
            .list_local_users("workspace_fixture")
            .expect("users")
            .is_empty()
    );
}

#[test]
fn local_user_inputs_fail_closed_at_each_field_boundary() {
    let (_temporary, repository) = repository();
    let event = local_user_event("audit_invalid", &fixture_user(), "user.created", 1);
    invalid(repository.authenticate_local_user_key("bad", 1));
    invalid(repository.authenticate_local_user_key(&checksum('a'), u64::MAX));
    invalid(repository.deactivate_local_user("", 1, &event));
    invalid(repository.list_local_users(""));
    for user in invalid_users() {
        invalid(repository.create_local_user(&user, &fixture_key(), &event));
    }
    for key in invalid_keys() {
        invalid(repository.create_local_user(&fixture_user(), &key, &event));
        invalid(repository.reset_local_user_login_key(&key, key.created_at_ms, &event));
    }
    let mut unlinked = fixture_key();
    unlinked.user_id = "user_other".to_owned();
    invalid(repository.create_local_user(&fixture_user(), &unlinked, &event));
    let mut skewed = fixture_key();
    skewed.created_at_ms = 2;
    skewed.expires_at_ms = 3;
    invalid(repository.create_local_user(&fixture_user(), &skewed, &event));
    invalid(repository.reset_local_user_login_key(&skewed, 1, &event));
    assert!(
        repository
            .list_local_users("workspace_fixture")
            .expect("users")
            .is_empty()
    );
}

fn invalid_users() -> Vec<LocalUserRecord> {
    let mut values = Vec::new();
    for field in 0..7 {
        let mut value = fixture_user();
        match field {
            0 => value.id.clear(),
            1 => value.display_name.clear(),
            2 => value.workspace_id.clear(),
            3 => value.email = Some(String::new()),
            4 => value.status = LocalUserStatus::Deactivated,
            5 => value.deactivated_at_ms = Some(0),
            _ => value.email = Some(" Mixed@Example.test".to_owned()),
        }
        values.push(value);
    }
    values
}

fn invalid_keys() -> Vec<LocalUserLoginKeyRecord> {
    let mut values = Vec::new();
    for field in 0..6 {
        let mut value = fixture_key();
        match field {
            0 => value.id.clear(),
            1 => value.token_prefix.clear(),
            2 => value.secret_hash = "bad".to_owned(),
            3 => value.expires_at_ms = value.created_at_ms,
            4 => value.last_used_at_ms = Some(1),
            _ => value.revoked_at_ms = Some(1),
        }
        values.push(value);
    }
    values
}
