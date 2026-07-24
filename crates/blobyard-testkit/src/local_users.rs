use crate::{ensure_equal, hash};
use blobyard_contract::{
    AuditValue, LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository,
    LocalUserStatus, NewAuditEvent, RepositoryError,
};

/// Runs the deterministic local-user contract against one populated workspace.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn local_user_conformance(
    repository: &dyn LocalUserRepository,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    ensure_equal(&repository.list_local_users(workspace_id)?, &Vec::new())?;
    let first = local_user(
        workspace_id,
        "user_first",
        Some("first@example.test".to_owned()),
        20,
    );
    let first_key = login_key("userkey_first", "user_first", 'f', 20);
    let created = local_user_event("audit_user_first", &first, "user.created", 20);
    repository.create_local_user(&first, &first_key, &created)?;
    assert_creation_failures(repository, workspace_id, &first, &first_key, &created)?;
    ensure_equal(
        &repository.authenticate_local_user_key(&first_key.secret_hash, 21)?,
        &first,
    )?;
    ensure_equal(
        &repository.authenticate_local_user_key(&first_key.secret_hash, 20)?,
        &first,
    )?;
    let missing = [
        repository.authenticate_local_user_key(&hash('9'), 21),
        repository.authenticate_local_user_key(&first_key.secret_hash, 1_000),
    ];
    if missing
        != [
            Err(RepositoryError::NotFound),
            Err(RepositoryError::NotFound),
        ]
    {
        return Err(RepositoryError::Unavailable);
    }
    let second_key = reset_and_verify(repository, &first, &first_key)?;
    deactivate_and_verify(repository, workspace_id, &first, &second_key)
}

fn assert_creation_failures(
    repository: &dyn LocalUserRepository,
    workspace_id: &str,
    first: &LocalUserRecord,
    first_key: &LocalUserLoginKeyRecord,
    created: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let duplicate_email = local_user(workspace_id, "user_email", first.email.clone(), 22);
    let duplicate_email_key = login_key("userkey_email", "user_email", '0', 22);
    let foreign = local_user("workspace_missing", "user_foreign", None, 22);
    let foreign_key = login_key("userkey_foreign", "user_foreign", '1', 22);
    let duplicate_hash = local_user(workspace_id, "user_hash", None, 22);
    let mut duplicate_hash_key = login_key("userkey_hash", "user_hash", '2', 22);
    duplicate_hash_key
        .secret_hash
        .clone_from(&first_key.secret_hash);
    let failures = [
        repository.create_local_user(first, first_key, created),
        repository.create_local_user(
            &duplicate_email,
            &duplicate_email_key,
            &local_user_event("audit_user_email", &duplicate_email, "user.created", 22),
        ),
        repository.create_local_user(
            &duplicate_hash,
            &duplicate_hash_key,
            &local_user_event("audit_user_hash", &duplicate_hash, "user.created", 22),
        ),
        repository.create_local_user(
            &foreign,
            &foreign_key,
            &local_user_event("audit_user_foreign", &foreign, "user.created", 22),
        ),
    ];
    let expected = [
        Err(RepositoryError::Conflict),
        Err(RepositoryError::Conflict),
        Err(RepositoryError::Conflict),
        Err(RepositoryError::NotFound),
    ];
    if failures != expected {
        return Err(RepositoryError::Unavailable);
    }
    ensure_equal(
        &repository.list_local_users(workspace_id)?,
        &vec![listing(first.clone(), Some(&first_key.token_prefix))],
    )
}

fn reset_and_verify(
    repository: &dyn LocalUserRepository,
    first: &LocalUserRecord,
    first_key: &LocalUserLoginKeyRecord,
) -> Result<LocalUserLoginKeyRecord, RepositoryError> {
    let replacement = login_key("userkey_second", "user_first", '3', 30);
    let orphan = login_key("userkey_orphan", "user_missing", '4', 30);
    let reset = local_user_event("audit_key_reset", first, "user.login_key_reset", 30);
    if repository.reset_local_user_login_key(&orphan, 30, &reset) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.reset_local_user_login_key(&replacement, 30, &reset)?;
    if repository.authenticate_local_user_key(&first_key.secret_hash, 31)
        != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    ensure_equal(
        &repository.authenticate_local_user_key(&replacement.secret_hash, 31)?,
        first,
    )?;
    Ok(replacement)
}

fn deactivate_and_verify(
    repository: &dyn LocalUserRepository,
    workspace_id: &str,
    first: &LocalUserRecord,
    first_active_key: &LocalUserLoginKeyRecord,
) -> Result<(), RepositoryError> {
    let second = local_user(workspace_id, "user_second", None, 35);
    let second_key = login_key("userkey_third", "user_second", '5', 35);
    let second_created = local_user_event("audit_user_second", &second, "user.created", 35);
    repository.create_local_user(&second, &second_key, &second_created)?;
    let deactivated = local_user_event("audit_user_deactivated", &second, "user.deactivated", 40);
    repository.deactivate_local_user(&second.id, 40, &deactivated)?;
    let failures = [
        repository.authenticate_local_user_key(&second_key.secret_hash, 41),
        repository.authenticate_local_user_key(&hash('8'), 41),
    ];
    if failures
        != [
            Err(RepositoryError::NotFound),
            Err(RepositoryError::NotFound),
        ]
        || repository.deactivate_local_user("user_missing", 41, &deactivated)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    let repeat = local_user_event("audit_user_redeactivated", &second, "user.deactivated", 41);
    let rekey = login_key("userkey_fourth", "user_second", '6', 41);
    let rekey_event = local_user_event("audit_key_rereset", &second, "user.login_key_reset", 41);
    let conflicts = [
        repository.deactivate_local_user(&second.id, 41, &repeat),
        repository.reset_local_user_login_key(&rekey, 41, &rekey_event),
    ];
    if conflicts
        != [
            Err(RepositoryError::Conflict),
            Err(RepositoryError::Conflict),
        ]
    {
        return Err(RepositoryError::Unavailable);
    }
    let mut tombstoned = second;
    tombstoned.status = LocalUserStatus::Deactivated;
    tombstoned.deactivated_at_ms = Some(40);
    ensure_equal(
        &repository.list_local_users(workspace_id)?,
        &vec![
            listing(tombstoned, None),
            listing(first.clone(), Some(&first_active_key.token_prefix)),
        ],
    )
}

/// Builds a stable local-user record for repository and server tests.
#[must_use]
pub fn local_user(
    workspace_id: &str,
    id: &str,
    email: Option<String>,
    created_at_ms: u64,
) -> LocalUserRecord {
    LocalUserRecord {
        id: id.to_owned(),
        workspace_id: workspace_id.to_owned(),
        display_name: "Fixture user".to_owned(),
        email,
        status: LocalUserStatus::Active,
        created_at_ms,
        deactivated_at_ms: None,
    }
}

/// Builds a stable hashed sign-in key record for repository and server tests.
#[must_use]
pub fn login_key(
    id: &str,
    user_id: &str,
    hash_character: char,
    created_at_ms: u64,
) -> LocalUserLoginKeyRecord {
    LocalUserLoginKeyRecord {
        id: id.to_owned(),
        user_id: user_id.to_owned(),
        token_prefix: format!("byuk_{hash_character}"),
        secret_hash: hash(hash_character),
        created_at_ms,
        expires_at_ms: 1_000,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

/// Builds the exact audit event the adapter must persist for one user lifecycle action.
#[must_use]
pub fn local_user_event(
    id: &str,
    user: &LocalUserRecord,
    action: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: user.workspace_id.clone(),
        actor: "token_fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{id}"),
        target_type: "local_user".to_owned(),
        metadata: vec![("userId".to_owned(), AuditValue::String(user.id.clone()))],
        created_at_ms,
    }
}

fn listing(user: LocalUserRecord, active_key_prefix: Option<&str>) -> LocalUserListing {
    LocalUserListing {
        user,
        active_key_prefix: active_key_prefix.map(str::to_owned),
    }
}
