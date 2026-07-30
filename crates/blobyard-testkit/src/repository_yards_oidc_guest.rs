use super::{YardConformanceRepository, audit, audit_count, oidc_authentication};
use crate::FixtureExecutionTracker;
use blobyard_contract::{RepositoryError, YardStartRecord};

pub(super) fn assert_guest_binding(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    email: &str,
    subject_id: &str,
    authenticated_at_ms: u64,
) -> Result<(), RepositoryError> {
    let mut tracker = FixtureExecutionTracker::new_oidc("testkit", "oidc-guest-repository");
    let authentication = oidc_authentication(
        "guest-subject",
        email,
        &first.yard.host_label,
        authenticated_at_ms,
    );
    let linked =
        repository.authenticate_yard_oidc_identity(&authentication, &audit("guest-linked"))?;
    if linked.yard_subject_id != subject_id
        || linked.normalized_email != email
        || audit_count(repository, "audit_oidc_guest-linked")? != 1
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-first-binding-reuses-one-accepted-guest",
        &serde_json::json!({"candidateKind": "accepted-guest", "candidateCount": 1}),
        &serde_json::json!({"bound": true, "provisioned": false, "auditEventCount": 1}),
    );
    assert_cross_tenant_denial(repository, first, &mut tracker, authenticated_at_ms + 1)?;
    assert_ambiguous_denial(
        repository,
        first,
        email,
        &mut tracker,
        authenticated_at_ms + 2,
    )?;
    tracker.finish()
}

fn assert_cross_tenant_denial(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
    at: u64,
) -> Result<(), RepositoryError> {
    let user = crate::local_user(
        "workspace_yard_foreign",
        "user_oidc_foreign",
        Some("foreign@example.test".to_owned()),
        120,
    );
    repository.create_local_user(
        &user,
        &crate::login_key("userkey_oidc_foreign", &user.id, '0', 120),
        &crate::local_user_event("audit_user_oidc_foreign", &user, "user.created", 120),
    )?;
    let authentication = oidc_authentication(
        "foreign-subject",
        "foreign@example.test",
        &first.yard.host_label,
        at,
    );
    if repository.authenticate_yard_oidc_identity(&authentication, &audit("foreign"))
        != Err(RepositoryError::NotFound)
        || audit_count(repository, "audit_oidc_foreign")? != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-cross-tenant-candidate-is-ignored",
        &serde_json::json!({"candidateWorkspaceMatchesYard": false}),
        &serde_json::json!({"admitted": false, "bindingCreated": false}),
    );
    Ok(())
}

fn assert_ambiguous_denial(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    email: &str,
    tracker: &mut FixtureExecutionTracker,
    at: u64,
) -> Result<(), RepositoryError> {
    let user = crate::local_user(
        "workspace_fixture",
        "user_oidc_ambiguous",
        Some(email.to_owned()),
        121,
    );
    repository.create_local_user(
        &user,
        &crate::login_key("userkey_oidc_ambiguous", &user.id, '1', 121),
        &crate::local_user_event("audit_user_oidc_ambiguous", &user, "user.created", 121),
    )?;
    let authentication =
        oidc_authentication("ambiguous-subject", email, &first.yard.host_label, at);
    if repository.authenticate_yard_oidc_identity(&authentication, &audit("ambiguous"))
        != Err(RepositoryError::NotFound)
        || audit_count(repository, "audit_oidc_ambiguous")? != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "oidc-ambiguous-candidates-deny-without-binding",
        &serde_json::json!({
            "candidateCount": 2,
            "candidateKinds": ["member", "accepted-guest"]
        }),
        &serde_json::json!({"admitted": false, "bindingCreated": false}),
    );
    Ok(())
}
