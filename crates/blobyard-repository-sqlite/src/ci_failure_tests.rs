use super::*;

#[test]
fn invalid_audit_and_foreign_project_roll_back_without_partial_state() {
    let (_temporary, repository) = repository();
    let valid_trust = trust("trust_fixture", None, 1);
    let mut invalid_trust = valid_trust.clone();
    invalid_trust.repository.clear();
    assert_eq!(
        repository.create_ci_trust(
            &invalid_trust,
            &event("ci.trust_created", "ci_trust", &invalid_trust.id, 1),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid_event = event("ci.trust_created", "ci_trust", &valid_trust.id, 1);
    invalid_event.actor = String::new();
    assert_eq!(
        repository.create_ci_trust(&valid_trust, &invalid_event),
        Err(RepositoryError::InvalidInput)
    );
    assert!(
        repository
            .list_ci_trusts("workspace_fixture")
            .expect("trusts")
            .is_empty()
    );

    let foreign = LocalCiTrustRecord {
        project_id: Some("project_foreign".to_owned()),
        ..trust("trust_foreign", None, 2)
    };
    assert_eq!(
        repository.create_ci_trust(
            &foreign,
            &event("ci.trust_created", "ci_trust", &foreign.id, 2),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn operation_specific_audit_contracts_fail_before_mutation() {
    let (_temporary, repository) = repository();
    let trust = trust("trust_fixture", None, 1);
    assert_eq!(
        repository.create_ci_trust(&trust, &event("ci.trust_revoked", "ci_trust", &trust.id, 1),),
        Err(RepositoryError::InvalidInput)
    );
    repository
        .create_ci_trust(&trust, &event("ci.trust_created", "ci_trust", &trust.id, 1))
        .expect("create trust");
    assert_eq!(
        repository.revoke_ci_trust(
            &trust.id,
            &trust.workspace_id,
            2,
            &event("ci.trust_created", "ci_trust", &trust.id, 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.mint_machine_session(
            &session(1, 10),
            &event("ci.trust_created", "project", "project_fixture", 10),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_ci_trusts("workspace_fixture"),
        Ok(vec![trust])
    );
}

#[test]
fn changed_trust_invalidates_both_machine_authentication_paths() {
    let (_temporary, repository, trust, session) = repository_with_trust_and_session();
    repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE ci_trusts SET allowed_actions = 'download' WHERE id = ?1",
            [&trust.id],
        )
        .expect("change trust actions");
    assert_eq!(
        repository.authenticate_api_token(&session.secret_hash, 11),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.authenticate_machine_session(&session.id, 11),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn orphaned_machine_session_fails_closed() {
    let (_temporary, repository, _trust, session) = repository_with_trust_and_session();
    {
        let connection = repository.test_connection().expect("connection");
        connection
            .execute_batch("PRAGMA foreign_keys = OFF; DELETE FROM ci_trusts WHERE id = 'trust_fixture'; PRAGMA foreign_keys = ON;")
            .expect("orphan machine session");
    }
    assert_eq!(
        repository.authenticate_machine_session(&session.id, 11),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.authenticate_api_token(&session.secret_hash, 11),
        Err(RepositoryError::NotFound)
    );
}
