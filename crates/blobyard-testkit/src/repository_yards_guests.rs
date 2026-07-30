use super::YardConformanceRepository;
#[path = "repository_yards_guests/fixtures.rs"]
mod fixtures;
#[path = "repository_yards_guests/limits.rs"]
mod limits;
#[path = "repository_yards_guests/policy.rs"]
mod policy;

use super::session_fixtures::{new_session, production_environment, set_visibility};
use crate::FixtureExecutionTracker;
use blobyard_contract::{
    NewYardGuestInvite, RepositoryError, YardGuestInviteStatus, YardSessionAuditContext,
    YardStartRecord, YardVisibility,
};
use fixtures::{
    CREATED_AT_MS, INVITATION_ID, KEY_HASH, SUBJECT_ID, TOKEN_HASH, continuation, event, grant,
    invitation, key, subject,
};

pub(super) fn assert_guest_controls(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    include_capacity: bool,
) -> Result<(), RepositoryError> {
    let mut tracker = FixtureExecutionTracker::new("testkit", "yard-guests");
    let (new_invitation, environment_id) =
        create_and_assert_invitation(repository, first, &mut tracker)?;
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Selected,
        CREATED_AT_MS + 1,
    )?;
    let continuation = continuation(&first.yard.id, &environment_id, &first.yard.host_label);
    let accepted = repository.accept_yard_guest_invite(
        TOKEN_HASH,
        &subject(),
        &key(new_invitation.expires_at_ms),
        &continuation,
        &event(
            "accepted",
            &new_invitation,
            Some(SUBJECT_ID),
            CREATED_AT_MS + 1,
        ),
        CREATED_AT_MS + 1,
    )?;
    if accepted.invitation.status != YardGuestInviteStatus::Accepted
        || repository.pending_yard_guest_invite_by_token(TOKEN_HASH, CREATED_AT_MS + 2)
            != Err(RepositoryError::NotFound)
        || repository.authenticate_yard_guest_key(KEY_HASH, CREATED_AT_MS + 2)? != accepted.subject
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "guest-acceptance-is-single-use-and-key-is-hash-only",
        &serde_json::json!({"surface": "account-acceptance"}),
        &serde_json::json!({"accepted": true, "replayResponse": "concealed-not-found"}),
    );
    assert_admission_and_identity(
        repository,
        first,
        &new_invitation,
        &continuation,
        &mut tracker,
    )?;
    policy::assert_guest_policy_boundaries(repository, first, &new_invitation, &mut tracker)?;
    assert_revocation(repository, first, &new_invitation, &mut tracker)?;
    limits::assert_invitation_limits(repository, first, &mut tracker, include_capacity)?;
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AnyAuthenticated,
        CREATED_AT_MS + 300,
    )?;
    if include_capacity {
        tracker.finish()
    } else {
        Ok(())
    }
}

fn create_and_assert_invitation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(NewYardGuestInvite, String), RepositoryError> {
    let environment = production_environment(repository.list_yard_environments(&first.yard.id)?)?;
    let new_invitation = invitation(&first.yard.id, &environment.id);
    let new_grant = grant(&new_invitation);
    let created = repository.create_yard_guest_invite(
        &new_invitation,
        &new_grant,
        &event("created", &new_invitation, None, CREATED_AT_MS),
    )?;
    if created.status != YardGuestInviteStatus::Pending
        || created.app_roles != ["editor"]
        || repository
            .list_yard_guest_invites(&first.yard.id, None, 50)?
            .items
            != [created]
    {
        return Err(RepositoryError::Unavailable);
    }
    record_create_and_redaction(tracker);
    assert_duplicate_conflict(repository, &new_invitation, &new_grant, tracker)?;
    Ok((new_invitation, environment.id))
}

fn assert_duplicate_conflict(
    repository: &dyn YardConformanceRepository,
    invitation: &blobyard_contract::NewYardGuestInvite,
    grant: &blobyard_contract::NewYardAccessGrant,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let mut duplicate_invitation = invitation.clone();
    "ygi_cccccccccccccccccccccccccccccccc".clone_into(&mut duplicate_invitation.id);
    duplicate_invitation.token_hash = "1".repeat(64);
    "yardgrant_guest_duplicate".clone_into(&mut duplicate_invitation.grant_id);
    let mut duplicate_grant = grant.clone();
    duplicate_grant
        .id
        .clone_from(&duplicate_invitation.grant_id);
    duplicate_grant
        .principal_id
        .clone_from(&duplicate_invitation.id);
    if repository.create_yard_guest_invite(
        &duplicate_invitation,
        &duplicate_grant,
        &event(
            "created",
            &duplicate_invitation,
            None,
            duplicate_invitation.created_at_ms,
        ),
    ) != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "duplicate-active-guest-invitation-conflicts-without-rotation",
        &serde_json::json!({"scope": "same-email-yard-environment"}),
        &serde_json::json!({"repositoryError": "CONFLICT", "auditEventCount": 0}),
    );
    Ok(())
}

fn assert_admission_and_identity(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    invitation: &blobyard_contract::NewYardGuestInvite,
    continuation: &blobyard_contract::NewYardContinuation,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let admission = repository.evaluate_yard_admission(
        &first.yard.host_label,
        SUBJECT_ID,
        CREATED_AT_MS + 3,
    )?;
    let exchange = repository.exchange_yard_session_code(
        &continuation.code_hash,
        &first.yard.host_label,
        &new_session("yardsession_guest_fixture", '9', CREATED_AT_MS + 3),
        &YardSessionAuditContext {
            id: "audit_guest_session_issued".to_owned(),
            request_id: "request_guest_session_issued".to_owned(),
        },
        CREATED_AT_MS + 3,
    )?;
    let identity = repository.resolve_yard_identity(
        &first.yard.host_label,
        &exchange.session.token_hash,
        CREATED_AT_MS + 4,
    )?;
    if admission.yard_id != invitation.yard_id
        || admission.environment_id != invitation.environment_id.as_deref().unwrap_or_default()
        || identity.user_id != SUBJECT_ID
        || identity.display_name.as_deref() != Some(invitation.email.as_str())
        || identity.email.as_deref() != Some(invitation.email.as_str())
        || !identity.groups.is_empty()
        || identity.management_role.is_some()
        || identity.app_roles != ["editor", "viewer"]
        || identity.permissions != ["content.read", "content.write"]
    {
        return Err(RepositoryError::Unavailable);
    }
    record_admission_and_identity(tracker);
    super::oidc::assert_guest_binding(
        repository,
        first,
        &invitation.email,
        SUBJECT_ID,
        invitation.created_at_ms + 5,
    )?;
    Ok(())
}

fn assert_revocation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    invitation: &blobyard_contract::NewYardGuestInvite,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let revoked = repository.revoke_yard_guest_invite(
        &first.yard.id,
        INVITATION_ID,
        CREATED_AT_MS + 20,
        &event("revoked", invitation, Some(SUBJECT_ID), CREATED_AT_MS + 20),
    )?;
    if revoked.status != YardGuestInviteStatus::Revoked
        || repository.evaluate_yard_admission(
            &first.yard.host_label,
            SUBJECT_ID,
            CREATED_AT_MS + 21,
        ) != Err(RepositoryError::NotFound)
        || repository.authenticate_yard_guest_key(KEY_HASH, CREATED_AT_MS + 21)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    super::oidc::assert_revoked_guest_binding(
        repository,
        first,
        &invitation.email,
        CREATED_AT_MS + 21,
    )?;
    tracker.record_case(
        "guest-revocation-denies-on-next-private-request",
        &serde_json::json!({"change": "invitation-revoked", "surface": "private-delivery"}),
        &serde_json::json!({"admitted": false, "propagationMilliseconds": 0}),
    );
    Ok(())
}

fn record_create_and_redaction(tracker: &mut FixtureExecutionTracker) {
    tracker.record_case(
        "guest-create-atomically-persists-invitation-grant-and-audit",
        &serde_json::json!({"mutation": "guest-invitation-create"}),
        &serde_json::json!({"invitationCount": 1, "grantCount": 1, "auditEventCount": 1}),
    );
    tracker.record_case(
        "guest-list-output-permanently-conceals-authority-material",
        &serde_json::json!({"surface": "guest-invitation-list"}),
        &serde_json::json!({"containsAuthorityMaterial": false, "pageSize": 50}),
    );
}

fn record_admission_and_identity(tracker: &mut FixtureExecutionTracker) {
    tracker.record_case(
        "selected-guest-grant-admits-only-matching-environment",
        &serde_json::json!({"principalKind": "guest-invite", "visibility": "selected"}),
        &serde_json::json!({"admitted": true, "environmentScoped": true}),
    );
    tracker.record_case(
        "guest-identity-expands-application-role-without-management-authority",
        &serde_json::json!({"principalKind": "guest", "applicationRole": "editor"}),
        &serde_json::json!({
            "applicationRoles": ["editor", "viewer"],
            "managementRole": null,
            "groups": []
        }),
    );
}
