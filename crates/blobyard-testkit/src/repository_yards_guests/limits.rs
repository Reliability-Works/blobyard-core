use super::{CREATED_AT_MS, YardConformanceRepository, event, grant};
use crate::FixtureExecutionTracker;
use blobyard_contract::{
    MAXIMUM_ACTIVE_YARD_GUEST_INVITES, NewYardGuestInvite, RepositoryError,
    YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS, YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS, YardStartRecord,
};

pub(super) fn assert_invitation_limits(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
    include_capacity: bool,
) -> Result<(), RepositoryError> {
    assert_invitation_lifetime_boundaries(repository, first, tracker)?;
    if include_capacity {
        assert_invitation_capacity(repository, first, tracker)
    } else {
        Ok(())
    }
}

fn assert_invitation_lifetime_boundaries(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    for (index, lifetime) in [
        (1, YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS),
        (2, YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS),
    ] {
        let invitation = boundary_invitation(first, index, lifetime);
        let created = repository.create_yard_guest_invite(
            &invitation,
            &grant(&invitation),
            &event("created", &invitation, None, invitation.created_at_ms),
        )?;
        if created.environment_id.is_some() {
            return Err(RepositoryError::Unavailable);
        }
        repository.revoke_yard_guest_invite(
            &first.yard.id,
            &invitation.id,
            invitation.created_at_ms + 1,
            &event("revoked", &invitation, None, invitation.created_at_ms + 1),
        )?;
    }
    tracker.record_case(
        "yard-wide-guest-invitations-accept-exact-lifetime-boundaries",
        &serde_json::json!({
            "environmentId": null,
            "lifetimeMilliseconds": [
                YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
                YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS
            ]
        }),
        &serde_json::json!({"created": true, "scope": "yard-wide"}),
    );
    assert_invalid_invitation_lifetimes(repository, first, tracker)
}

fn assert_invalid_invitation_lifetimes(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    for (index, lifetime) in [
        (3, YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS - 1),
        (4, YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS + 1),
    ] {
        let invitation = boundary_invitation(first, index, lifetime);
        if repository.create_yard_guest_invite(
            &invitation,
            &grant(&invitation),
            &event("created", &invitation, None, invitation.created_at_ms),
        ) != Err(RepositoryError::InvalidInput)
        {
            return Err(RepositoryError::Unavailable);
        }
    }
    tracker.record_case(
        "guest-invitation-lifetimes-outside-five-minutes-and-thirty-days-fail",
        &serde_json::json!({
            "lifetimeMilliseconds": [
                YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS - 1,
                YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS + 1
            ]
        }),
        &serde_json::json!({"repositoryError": "INVALID_INPUT", "authorityCreated": false}),
    );
    Ok(())
}

fn assert_invitation_capacity(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let maximum_active = MAXIMUM_ACTIVE_YARD_GUEST_INVITES as u64;
    for index in 0..maximum_active {
        let invitation = capacity_invitation(first, index);
        repository.create_yard_guest_invite(
            &invitation,
            &grant(&invitation),
            &event("created", &invitation, None, invitation.created_at_ms),
        )?;
    }
    let overflow = capacity_invitation(first, maximum_active);
    if repository.create_yard_guest_invite(
        &overflow,
        &grant(&overflow),
        &event("created", &overflow, None, overflow.created_at_ms),
    ) != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "guest-invitation-capacity-allows-exactly-one-hundred-active-authorities",
        &serde_json::json!({"activeInvitations": 100, "nextMutation": "create"}),
        &serde_json::json!({"repositoryError": "CONFLICT", "activeInvitations": 100}),
    );
    Ok(())
}

fn boundary_invitation(
    first: &YardStartRecord,
    index: u64,
    lifetime_ms: u64,
) -> NewYardGuestInvite {
    let created_at_ms = CREATED_AT_MS + 30 + index;
    invitation_fixture(
        first,
        format!("ygi_b{index:031x}"),
        format!("yardgrant_boundary_{index}"),
        format!("boundary{index}@example.test"),
        format!("{:064x}", index + 10),
        created_at_ms,
        lifetime_ms,
    )
}

fn capacity_invitation(first: &YardStartRecord, index: u64) -> NewYardGuestInvite {
    let created_at_ms = CREATED_AT_MS + 100 + index;
    invitation_fixture(
        first,
        format!("ygi_c{index:031x}"),
        format!("yardgrant_capacity_{index}"),
        format!("capacity{index}@example.test"),
        format!("{:064x}", index + 1_000),
        created_at_ms,
        YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
    )
}

fn invitation_fixture(
    first: &YardStartRecord,
    id: String,
    grant_id: String,
    email: String,
    token_hash: String,
    created_at_ms: u64,
    lifetime_ms: u64,
) -> NewYardGuestInvite {
    NewYardGuestInvite {
        id,
        workspace_id: first.yard.workspace_id.clone(),
        project_id: first.yard.project_id.clone(),
        yard_id: first.yard.id.clone(),
        environment_id: None,
        email,
        token_hash,
        grant_id,
        created_at_ms,
        expires_at_ms: created_at_ms + lifetime_ms,
    }
}
