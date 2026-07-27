use super::super::fixtures::granted_event;
use super::{CREATED_AT_MS, SUBJECT_ID, YardConformanceRepository};
use crate::FixtureExecutionTracker;
use blobyard_contract::{
    NewYardAccessGrant, NewYardGuestInvite, RepositoryError, YardAccessPrincipalKind,
    YardStartRecord, YardVisibility,
};

pub(super) fn assert_guest_policy_boundaries(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    invitation: &NewYardGuestInvite,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    assert_guest_visibility_boundaries(repository, first, tracker)?;
    assert_guest_principal_boundaries(repository, first, invitation, tracker)?;
    super::set_visibility(
        repository,
        &first.yard.id,
        "workspace",
        YardVisibility::Selected,
        CREATED_AT_MS + 13,
    )
}

fn assert_guest_visibility_boundaries(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    super::set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AuthenticatedLink,
        CREATED_AT_MS + 5,
    )?;
    if repository
        .evaluate_yard_admission(&first.yard.host_label, SUBJECT_ID, CREATED_AT_MS + 6)
        .is_err()
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "authenticated-link-admits-a-matching-guest-invitation",
        &serde_json::json!({
            "principalKind": "guest",
            "visibility": "authenticated-link"
        }),
        &serde_json::json!({"admitted": true, "authority": "guest-invitation"}),
    );

    for (from, visibility, at) in [
        (
            "authenticated-link",
            YardVisibility::Owner,
            CREATED_AT_MS + 7,
        ),
        ("owner", YardVisibility::Workspace, CREATED_AT_MS + 9),
    ] {
        super::set_visibility(repository, &first.yard.id, from, visibility, at)?;
        if repository.evaluate_yard_admission(&first.yard.host_label, SUBJECT_ID, at + 1)
            != Err(RepositoryError::NotFound)
        {
            return Err(RepositoryError::Unavailable);
        }
    }
    tracker.record_case(
        "guest-does-not-inherit-owner-or-workspace-admission",
        &serde_json::json!({
            "principalKind": "guest",
            "visibility": ["owner", "workspace"]
        }),
        &serde_json::json!({"admitted": false, "responseClass": "concealed-not-found"}),
    );
    Ok(())
}

fn assert_guest_principal_boundaries(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    invitation: &NewYardGuestInvite,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    for (kind, suffix) in [
        (YardAccessPrincipalKind::User, "direct"),
        (YardAccessPrincipalKind::Group, "group"),
    ] {
        let grant = NewYardAccessGrant {
            id: format!("yardgrant_guest_{suffix}"),
            yard_id: invitation.yard_id.clone(),
            environment_id: invitation.environment_id.clone(),
            principal_kind: kind,
            principal_id: SUBJECT_ID.to_owned(),
            app_roles: Vec::new(),
            created_at_ms: CREATED_AT_MS + 11,
            created_by_principal: "fixture".to_owned(),
            expires_at_ms: None,
        };
        if repository.insert_yard_access_grant(
            &grant,
            &granted_event(&first.yard.id, &grant, CREATED_AT_MS + 11),
        ) != Err(RepositoryError::InvalidInput)
        {
            return Err(RepositoryError::Unavailable);
        }
        tracker.record_case(
            &format!("guest-subject-cannot-inherit-{suffix}-principal-authority"),
            &serde_json::json!({
                "guestSubjectId": SUBJECT_ID,
                "attemptedPrincipalKind": kind.as_str()
            }),
            &serde_json::json!({"admitted": false, "repositoryError": "INVALID_INPUT"}),
        );
    }
    Ok(())
}
