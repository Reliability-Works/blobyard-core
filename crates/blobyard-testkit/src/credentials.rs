use blobyard_contract::{
    AuditValue, CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent,
    RepositoryError,
};

/// Runs the deterministic local credential contract against one empty adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn credential_conformance(
    repository: &dyn CredentialRepository,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let bootstrap = hash('b');
    let api_hash = hash('a');
    if !repository.install_bootstrap(&bootstrap)? || repository.install_bootstrap(&hash('c'))? {
        return Err(RepositoryError::Unavailable);
    }
    let token = token(workspace_id, api_hash.clone());
    let session = cli_session_record(&token, "0.1.12");
    repository.exchange_bootstrap(&bootstrap, &token, &session)?;
    let mut authenticated = token.clone();
    authenticated.last_used_at_ms = Some(2);
    ensure_equal(
        &repository.authenticate_api_token(&api_hash, 2)?,
        &authenticated,
    )?;
    let mut used_session = session.clone();
    used_session.last_used_at_ms = Some(2);
    ensure_equal(
        &repository.list_cli_sessions(workspace_id)?,
        &vec![used_session],
    )?;
    let created = created_token(workspace_id);
    let create_event = credential_event(
        "audit_token_created",
        workspace_id,
        "api_token.created",
        &created.id,
        5,
    );
    repository.create_api_token(&created, &create_event)?;
    assert_creation_failures(repository, workspace_id, &created, &create_event)?;
    let mut used = created.clone();
    used.last_used_at_ms = Some(6);
    ensure_equal(
        &repository.authenticate_api_token(&created.secret_hash, 6)?,
        &used,
    )?;
    ensure_equal(
        &repository.authenticate_api_token(&created.secret_hash, 5)?,
        &used,
    )?;
    if repository.authenticate_api_token(&created.secret_hash, 10) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    ensure_equal(&repository.list_api_tokens()?, &vec![used])?;
    if repository.exchange_bootstrap(&bootstrap, &token, &session) != Err(RepositoryError::NotFound)
        || repository.install_bootstrap(&hash('d'))?
    {
        return Err(RepositoryError::Unavailable);
    }
    revoke_and_verify(repository, workspace_id, &api_hash)
}

fn assert_creation_failures(
    repository: &dyn CredentialRepository,
    workspace_id: &str,
    created: &LocalApiTokenRecord,
    create_event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let duplicate_hash = LocalApiTokenRecord {
        id: "token_duplicate_hash".to_owned(),
        ..created.clone()
    };
    let duplicate_hash_event = credential_event(
        "audit_token_duplicate_hash",
        workspace_id,
        "api_token.created",
        &duplicate_hash.id,
        5,
    );
    if repository.create_api_token(created, create_event) != Err(RepositoryError::Conflict)
        || repository.create_api_token(&duplicate_hash, &duplicate_hash_event)
            != Err(RepositoryError::Conflict)
        || repository.authenticate_api_token(&created.secret_hash, 4)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn revoke_and_verify(
    repository: &dyn CredentialRepository,
    workspace_id: &str,
    api_hash: &str,
) -> Result<(), RepositoryError> {
    let token = token(workspace_id, api_hash.to_owned());
    let created = created_token(workspace_id);
    let mut used = created.clone();
    used.last_used_at_ms = Some(6);
    let revoke_session = cli_session_revoked_event(workspace_id, "session_fixture", 3);
    let created_revoke_event = credential_event(
        "audit_token_revoked_created",
        workspace_id,
        "api_token.revoked",
        &created.id,
        11,
    );
    repository.revoke_cli_session("session_fixture", workspace_id, 3, &revoke_session)?;
    repository.revoke_api_token(&created.id, 11, &created_revoke_event)?;
    if repository.authenticate_api_token(api_hash, 4) != Err(RepositoryError::NotFound)
        || repository.revoke_cli_session("session_fixture", workspace_id, 4, &revoke_session)
            != Err(RepositoryError::Conflict)
        || repository.revoke_api_token(&token.id, 4, &created_revoke_event)
            != Err(RepositoryError::NotFound)
        || repository.revoke_api_token(&created.id, 12, &created_revoke_event)
            != Err(RepositoryError::Conflict)
        || repository.revoke_api_token("token_missing", 12, &created_revoke_event)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    let mut revoked_created_token = used;
    revoked_created_token.revoked_at_ms = Some(11);
    ensure_equal(&repository.list_api_tokens()?, &vec![revoked_created_token])?;
    ensure_equal(&repository.list_cli_sessions(workspace_id)?, &Vec::new())
}

/// Builds a stable, non-secret CLI session record for repository and server tests.
#[must_use]
pub fn cli_session_record(token: &LocalApiTokenRecord, version: &str) -> LocalCliSessionRecord {
    LocalCliSessionRecord {
        id: "session_fixture".to_owned(),
        token_id: token.id.clone(),
        workspace_id: token.workspace_id.clone(),
        name: token.name.clone(),
        platform: "test".to_owned(),
        version: version.to_owned(),
        created_at_ms: token.created_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn token(workspace_id: &str, secret_hash: String) -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture operator".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash,
        scopes: vec!["object:read".to_owned(), "object:write".to_owned()],
        workspace_id: workspace_id.to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 1_000,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn created_token(workspace_id: &str) -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: "token_created".to_owned(),
        name: "Fixture automation".to_owned(),
        token_prefix: "byd_pat_fixture".to_owned(),
        secret_hash: hash('e'),
        scopes: vec!["object:read".to_owned()],
        workspace_id: workspace_id.to_owned(),
        project_id: None,
        created_at_ms: 5,
        expires_at_ms: 10,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn credential_event(
    id: &str,
    workspace_id: &str,
    action: &str,
    token_id: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: workspace_id.to_owned(),
        actor: "token_fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{id}"),
        target_type: "api_token".to_owned(),
        metadata: vec![(
            "tokenId".to_owned(),
            AuditValue::String(token_id.to_owned()),
        )],
        created_at_ms,
    }
}

/// Builds the canonical non-secret audit fixture for revoking a CLI session.
#[must_use]
pub fn cli_session_revoked_event(
    workspace_id: &str,
    session_id: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_session_revoked".to_owned(),
        workspace_id: workspace_id.to_owned(),
        actor: "token_fixture".to_owned(),
        action: "cli.session_revoked".to_owned(),
        request_id: "request_session_revoked".to_owned(),
        target_type: "cli_session".to_owned(),
        metadata: vec![(
            "sessionId".to_owned(),
            AuditValue::String(session_id.to_owned()),
        )],
        created_at_ms,
    }
}

fn ensure_equal<T: Eq>(actual: &T, expected: &T) -> Result<(), RepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}
