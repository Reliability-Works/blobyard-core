use super::YardConformanceRepository;
use super::session_fixtures::{issue_session, set_visibility};
use crate::{FixtureExecutionTracker, hash};
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YARD_OIDC_ATTEMPT_LIFETIME_MS,
    YARD_OIDC_IDENTITY_LINKED_ACTION, YardOidcAuditContext, YardStartRecord, YardVisibility,
};

#[path = "repository_yards_oidc_guest.rs"]
mod guest;

const ISSUER: &str = "https://identity.example.test/";

pub(super) fn assert_member_and_attempt_controls(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    let mut tracker = FixtureExecutionTracker::new_oidc("testkit", "oidc-repository");
    assert_attempt_lifecycle(repository, first, &mut tracker)?;
    set_visibility(
        repository,
        &first.yard.id,
        "public",
        YardVisibility::AnyAuthenticated,
        80,
    )?;
    assert_member_binding(repository, first, &mut tracker)?;
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Public,
        99,
    )?;
    tracker.finish()
}

fn assert_attempt_lifecycle(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let attempt = oidc_attempt(first, '1', '2', 50);
    repository.create_yard_oidc_attempt(&attempt)?;
    if repository.create_yard_oidc_attempt(&attempt) != Err(RepositoryError::Conflict) {
        return Err(RepositoryError::Unavailable);
    }
    let claimed = repository.claim_yard_oidc_attempt(&attempt.state_hash, 51)?;
    if claimed.attempt != attempt
        || claimed.claimed_at_ms != Some(51)
        || repository.claim_yard_oidc_attempt(&attempt.state_hash, 52)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-attempt-is-hash-only-and-single-claim",
        &serde_json::json!({"stateShape": "byos-plus-64-lower-hex"}),
        &serde_json::json!({"persistedRawState": false, "claimCount": 1}),
    );
    let expired = oidc_attempt(first, '3', '4', 60);
    repository.create_yard_oidc_attempt(&expired)?;
    if repository.claim_yard_oidc_attempt(&expired.state_hash, expired.expires_at_ms)
        != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-attempt-expires-after-ten-minutes",
        &serde_json::json!({"surface": "oidc-attempt"}),
        &serde_json::json!({"lifetimeMilliseconds": YARD_OIDC_ATTEMPT_LIFETIME_MS}),
    );
    Ok(())
}

fn assert_member_binding(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let authentication = oidc_authentication(
        "member-subject",
        "member@example.test",
        &first.yard.host_label,
        81,
    );
    let linked =
        repository.authenticate_yard_oidc_identity(&authentication, &audit("member-linked"))?;
    if linked.yard_subject_id != "user_fixture"
        || linked.workspace_id != first.yard.workspace_id
        || linked.normalized_email != "member@example.test"
        || audit_count(repository, "audit_oidc_member-linked")? != 1
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-first-binding-reuses-one-active-member",
        &serde_json::json!({"candidateKind": "member", "candidateCount": 1}),
        &serde_json::json!({"bound": true, "provisioned": false, "auditEventCount": 1}),
    );
    let returning = NewYardOidcAuthentication {
        authenticated_at_ms: 82,
        ..authentication.clone()
    };
    let reused =
        repository.authenticate_yard_oidc_identity(&returning, &audit("member-returning"))?;
    if reused.yard_subject_id != linked.yard_subject_id
        || reused.last_authenticated_at_ms != 82
        || audit_count(repository, "audit_oidc_member-returning")? != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-returning-binding-reuses-exact-issuer-subject-workspace",
        &serde_json::json!({"issuerMatches": true, "subjectMatches": true, "workspaceMatches": true}),
        &serde_json::json!({"bindingCreated": false, "lastAuthenticatedUpdated": true}),
    );
    assert_email_revocation(repository, first, tracker, authentication, returning)?;
    assert_zero_candidate(repository, first, tracker)
}

fn assert_email_revocation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
    authentication: NewYardOidcAuthentication,
    returning: NewYardOidcAuthentication,
) -> Result<(), RepositoryError> {
    let session = issue_session(repository, first, "oidc-drift", '4', 83)?;
    let drifting = NewYardOidcAuthentication {
        normalized_email: Some("changed@example.test".to_owned()),
        authenticated_at_ms: 85,
        ..authentication
    };
    if repository.authenticate_yard_oidc_identity(&drifting, &audit("member-drift"))
        != Err(RepositoryError::NotFound)
        || repository
            .list_yard_sessions(&first.yard.id)?
            .into_iter()
            .find(|listing| listing.session.id == session.id)
            .is_none_or(|listing| listing.session.revoked_at_ms != Some(85))
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-verified-email-drift-revokes-subject-sessions",
        &serde_json::json!({"verifiedEmailMatchesBinding": false}),
        &serde_json::json!({"admitted": false, "sessionsRevoked": true}),
    );
    let missing_email_session = issue_session(repository, first, "oidc-missing-email", '5', 86)?;
    let missing_email = NewYardOidcAuthentication {
        normalized_email: None,
        authenticated_at_ms: 88,
        ..returning
    };
    if repository.authenticate_yard_oidc_identity(&missing_email, &audit("member-missing-email"))
        != Err(RepositoryError::NotFound)
        || repository
            .list_yard_sessions(&first.yard.id)?
            .into_iter()
            .find(|listing| listing.session.id == missing_email_session.id)
            .is_none_or(|listing| listing.session.revoked_at_ms != Some(88))
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-missing-verified-email-revokes-subject-sessions",
        &serde_json::json!({"verifiedEmail": null, "existingBinding": true}),
        &serde_json::json!({"admitted": false, "sessionsRevoked": true}),
    );
    Ok(())
}

fn assert_zero_candidate(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let missing = oidc_authentication(
        "missing-subject",
        "missing@example.test",
        &first.yard.host_label,
        89,
    );
    if repository.authenticate_yard_oidc_identity(&missing, &audit("missing"))
        != Err(RepositoryError::NotFound)
        || audit_count(repository, "audit_oidc_missing")? != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    let missing_email = NewYardOidcAuthentication {
        provider_subject: "missing-email-subject".to_owned(),
        normalized_email: None,
        authenticated_at_ms: 90,
        ..missing.clone()
    };
    let unknown_host = NewYardOidcAuthentication {
        provider_subject: "unknown-host-subject".to_owned(),
        normalized_email: Some("missing@example.test".to_owned()),
        host_label: "unknown-oidc-host".to_owned(),
        authenticated_at_ms: 91,
        ..missing
    };
    if repository.authenticate_yard_oidc_identity(&missing_email, &audit("missing-email"))
        != Err(RepositoryError::NotFound)
        || repository.authenticate_yard_oidc_identity(&unknown_host, &audit("unknown-host"))
            != Err(RepositoryError::NotFound)
        || audit_count(repository, "audit_oidc_missing-email")? != 0
        || audit_count(repository, "audit_oidc_unknown-host")? != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-zero-candidate-denies-without-provisioning",
        &serde_json::json!({"candidateCount": 0}),
        &serde_json::json!({"admitted": false, "provisioned": false}),
    );
    Ok(())
}

pub(super) fn assert_guest_binding(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    email: &str,
    subject_id: &str,
    authenticated_at_ms: u64,
) -> Result<(), RepositoryError> {
    guest::assert_guest_binding(repository, first, email, subject_id, authenticated_at_ms)
}

pub(super) fn assert_revoked_guest_binding(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    email: &str,
    authenticated_at_ms: u64,
) -> Result<(), RepositoryError> {
    let authentication = oidc_authentication(
        "guest-subject",
        email,
        &first.yard.host_label,
        authenticated_at_ms,
    );
    if repository.authenticate_yard_oidc_identity(&authentication, &audit("guest-revoked"))
        == Err(RepositoryError::NotFound)
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn oidc_attempt(
    first: &YardStartRecord,
    state: char,
    continuation: char,
    created_at_ms: u64,
) -> NewYardOidcAttempt {
    NewYardOidcAttempt {
        state_hash: hash(state),
        continuation_hash: hash(continuation),
        host_label: first.yard.host_label.clone(),
        return_path: "/reports".to_owned(),
        created_at_ms,
        expires_at_ms: created_at_ms + YARD_OIDC_ATTEMPT_LIFETIME_MS,
    }
}

fn oidc_authentication(
    provider_subject: &str,
    normalized_email: &str,
    host_label: &str,
    authenticated_at_ms: u64,
) -> NewYardOidcAuthentication {
    NewYardOidcAuthentication {
        issuer: ISSUER.to_owned(),
        provider_subject: provider_subject.to_owned(),
        normalized_email: Some(normalized_email.to_owned()),
        host_label: host_label.to_owned(),
        authenticated_at_ms,
    }
}

fn audit(suffix: &str) -> YardOidcAuditContext {
    YardOidcAuditContext {
        id: format!("audit_oidc_{suffix}"),
        request_id: format!("request_oidc_{suffix}"),
    }
}

fn audit_count(
    repository: &dyn YardConformanceRepository,
    event_id: &str,
) -> Result<usize, RepositoryError> {
    repository
        .list_audit("workspace_fixture", None, 100)
        .map(|page| {
            page.items
                .iter()
                .filter(|event| {
                    event.action == YARD_OIDC_IDENTITY_LINKED_ACTION && event.id == event_id
                })
                .count()
        })
}
