use super::*;

fn assert_invalid_exchange(value: &NewMachineSession) {
    assert_eq!(
        ci_validation::validate_exchange(value).map(|_times| ()),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn public_ci_boundaries_reject_invalid_identifiers_and_times() {
    let (_temporary, repository) = repository();
    let trust = trust("trust_fixture", None, 1);
    let valid_event = event("ci.trust_revoked", "ci_trust", &trust.id, 2);
    assert_eq!(
        repository.list_ci_trusts(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_ci_trust("", &trust.workspace_id, 2, &valid_event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_ci_trust(&trust.id, "", 2, &valid_event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_ci_trust(&trust.id, &trust.workspace_id, u64::MAX, &valid_event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.mint_machine_session(
            &NewMachineSession {
                id: String::new(),
                ..session(1, 10)
            },
            &event("ci.token_minted", "project", "project_fixture", 10),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.authenticate_machine_session("", 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.authenticate_machine_session("machine_fixture", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn malformed_persisted_trust_fails_closed_in_list_and_exchange() {
    let (_temporary, repository, trust) = repository_with_trust();
    repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE ci_trusts SET allowed_actions = 'invalid' WHERE id = ?1",
            [&trust.id],
        )
        .expect("corrupt trust actions");
    assert_eq!(
        repository.list_ci_trusts(&trust.workspace_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        repository.mint_machine_session(
            &session(1, 10),
            &event("ci.token_minted", "project", "project_fixture", 10),
        ),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn trust_validation_rejects_every_malformed_boundary() {
    let valid = trust("trust_fixture", Some("project_fixture"), 1);
    let mut variants = Vec::new();
    let mut id = valid.clone();
    id.id.clear();
    variants.push(id);
    let mut project = valid.clone();
    project.project_id = Some(String::new());
    variants.push(project);
    let mut environment = valid.clone();
    environment.environment = Some(String::new());
    variants.push(environment);
    let mut revoked = valid.clone();
    revoked.revoked_at_ms = Some(2);
    variants.push(revoked);
    let mut time = valid.clone();
    time.created_at_ms = u64::MAX;
    variants.push(time);
    for repository in [
        "invalid",
        "Owner/repository",
        "-owner/repository",
        "owner_/repository",
        "owner/repository_",
        "owner/repository!",
    ] {
        let mut value = valid.clone();
        value.repository = repository.to_owned();
        variants.push(value);
    }
    let mut workflow = valid.clone();
    workflow.workflow_path = "release.yml".to_owned();
    variants.push(workflow);
    for path in [
        ".github/workflows/release.txt",
        ".github/workflows/nested/release.yml",
    ] {
        let mut workflow = valid.clone();
        workflow.workflow_path = path.to_owned();
        variants.push(workflow);
    }
    let mut workflow_ref = valid.clone();
    workflow_ref.workflow_ref = "main".to_owned();
    variants.push(workflow_ref);
    for git_ref in ["refs/heads/", "refs/heads/a..b", "refs/heads/bad!"] {
        let mut workflow_ref = valid.clone();
        workflow_ref.workflow_ref = git_ref.to_owned();
        variants.push(workflow_ref);
    }
    let mut glob = valid.clone();
    glob.allowed_ref_glob = "refs/heads/**".to_owned();
    variants.push(glob);
    for ref_glob in ["refs/heads/", "refs/heads/a..b", "refs/heads/bad!"] {
        let mut glob = valid.clone();
        glob.allowed_ref_glob = ref_glob.to_owned();
        variants.push(glob);
    }
    let mut empty_actions = valid.clone();
    empty_actions.allowed_actions.clear();
    variants.push(empty_actions);
    let mut duplicate_actions = valid.clone();
    duplicate_actions.allowed_actions = vec![CiAction::Upload, CiAction::Upload];
    variants.push(duplicate_actions);
    let mut too_many_actions = valid.clone();
    too_many_actions.allowed_actions = vec![
        CiAction::Upload,
        CiAction::Download,
        CiAction::Upload,
        CiAction::Download,
        CiAction::Upload,
    ];
    variants.push(too_many_actions);
    for variant in variants {
        assert_eq!(
            ci_validation::validate_trust(&variant),
            Err(RepositoryError::InvalidInput)
        );
    }

    let mut sha_ref = valid;
    sha_ref.workflow_ref = "a".repeat(40);
    sha_ref.environment = Some("production".to_owned());
    assert_eq!(ci_validation::validate_trust(&sha_ref), Ok(1));
    let mut yaml_workflow = sha_ref;
    yaml_workflow.workflow_path = ".github/workflows/release.yaml".to_owned();
    yaml_workflow.allowed_ref_glob = "refs/tags/release-*".to_owned();
    assert_eq!(ci_validation::validate_trust(&yaml_workflow), Ok(1));
}

#[test]
fn exchange_validation_rejects_every_malformed_boundary() {
    let valid = session(1, 10);
    let mut variants = Vec::new();
    let mut bad_hash = valid.clone();
    bad_hash.oidc_token_hash = "invalid".to_owned();
    variants.push(bad_hash);
    let mut bad_secret = valid.clone();
    bad_secret.secret_hash = "invalid".to_owned();
    variants.push(bad_secret);
    let mut project = valid.clone();
    project.project.clear();
    variants.push(project);
    let mut workspace = valid.clone();
    workspace.workspace = Some(String::new());
    variants.push(workspace);
    let mut environment = valid.clone();
    environment.identity.environment = Some(String::new());
    variants.push(environment);
    let mut actions = valid.clone();
    actions.actions.clear();
    variants.push(actions);
    let mut id = valid.clone();
    id.id = "session_fixture".to_owned();
    variants.push(id);
    let mut token_prefix = valid.clone();
    token_prefix.token_prefix.clear();
    variants.push(token_prefix);
    let mut expired = valid.clone();
    expired.identity.expires_at_ms = expired.now_ms;
    variants.push(expired);
    let mut too_long = valid.clone();
    too_long.identity.expires_at_ms = too_long.now_ms + ci_validation::MACHINE_SESSION_TTL_MS + 1;
    variants.push(too_long);
    let mut repository = valid.clone();
    repository.identity.repository = "Owner/repository".to_owned();
    variants.push(repository);
    let mut git_ref = valid.clone();
    git_ref.identity.git_ref = "main".to_owned();
    variants.push(git_ref);
    let mut workflow = valid.clone();
    workflow.identity.workflow_path = "release.yml".to_owned();
    variants.push(workflow);
    let mut workflow_ref = valid.clone();
    workflow_ref.identity.workflow_ref = "main".to_owned();
    variants.push(workflow_ref);
    let mut token_name = valid.clone();
    token_name.identity.run_id = "1".repeat(512);
    variants.push(token_name);
    for variant in variants {
        assert_invalid_exchange(&variant);
    }

    let mut overflow = valid;
    overflow.now_ms = u64::MAX;
    overflow.identity.expires_at_ms = u64::MAX;
    assert_invalid_exchange(&overflow);
    let mut expiry_overflow = session(2, i64::MAX as u64);
    expiry_overflow.identity.expires_at_ms = u64::MAX;
    assert_invalid_exchange(&expiry_overflow);
}

#[test]
fn audit_and_ref_glob_validation_are_exact() {
    let trust = trust("trust_fixture", None, 1);
    let valid = event("ci.trust_created", "ci_trust", &trust.id, 1);
    assert_eq!(
        ci_validation::validate_event(
            &valid,
            "ci.trust_created",
            "ci_trust",
            &trust.id,
            &trust.repository,
            &trust.workspace_id,
            1,
        ),
        Ok(())
    );
    assert_eq!(
        ci_validation::validate_event(
            &valid,
            "ci.trust_revoked",
            "ci_trust",
            &trust.id,
            &trust.repository,
            &trust.workspace_id,
            1,
        ),
        Err(RepositoryError::InvalidInput)
    );
    for (value, glob, expected) in [
        ("refs/heads/main", "refs/heads/main", true),
        ("refs/tags/main", "refs/heads/*", false),
        ("refs/heads/main", "refs/heads/*", true),
        ("refs/heads/main", "refs/heads/release-*", false),
        ("refs/heads/release/v1", "refs/*/release/*", true),
        ("refs/heads/main", "refs/*/release/*", false),
        ("refs/heads/release/v1", "refs/*/*/v1", true),
        ("refs/heads/main", "refs/*/*/v1", false),
        ("prefix-middle-suffix", "prefix-*-suffix", true),
        ("anything", "*", true),
    ] {
        assert_eq!(ci_validation::ref_matches(value, glob), expected);
    }
}

#[test]
fn trust_query_parameter_failures_are_unavailable() {
    let (_temporary, repository) = repository();
    let result = {
        let connection = repository.test_connection().expect("connection");
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM ci_trusts WHERE repository = ?1",
                super::super::rows::CI_TRUST_COLUMNS
            ))
            .expect("trust query");
        let result = super::super::ci_records::query_trusts(&mut statement, &[]);
        drop(statement);
        drop(connection);
        result
    };
    assert_eq!(result, Err(RepositoryError::Unavailable));
}
