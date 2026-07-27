#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::{Fixture, grant, invitation};
use blobyard_contract::{
    RepositoryError, YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS, YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
    YardGuestRepository,
};

use super::tests::events::event;

#[test]
fn guest_timestamp_outside_sqlite_range_is_rejected() {
    let overflow = Fixture::new();
    let mut overflow_invitation = invitation();
    overflow_invitation.created_at_ms = i64::MAX.cast_unsigned() + 1;
    overflow_invitation.expires_at_ms =
        overflow_invitation.created_at_ms + YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS;
    let mut overflow_grant = grant();
    overflow_grant.created_at_ms = overflow_invitation.created_at_ms;
    overflow_grant.expires_at_ms = Some(overflow_invitation.expires_at_ms);
    assert_eq!(
        overflow.repository.create_yard_guest_invite(
            &overflow_invitation,
            &overflow_grant,
            &event(
                "created",
                &overflow_invitation,
                None,
                overflow_invitation.created_at_ms,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn guest_expiry_outside_sqlite_range_is_rejected_after_valid_creation_time() {
    let fixture = Fixture::new();
    let mut candidate = invitation();
    candidate.created_at_ms = i64::MAX.cast_unsigned() - YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS + 1;
    candidate.expires_at_ms = candidate.created_at_ms + YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS;
    let mut candidate_grant = grant();
    candidate_grant.created_at_ms = candidate.created_at_ms;
    candidate_grant.expires_at_ms = Some(candidate.expires_at_ms);
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &candidate,
            &candidate_grant,
            &event("created", &candidate, None, candidate.created_at_ms),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn guest_invitation_lifetime_boundaries_are_inclusive_and_exact() {
    for lifetime in [
        YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
        YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS,
    ] {
        let fixture = Fixture::new();
        let (candidate, candidate_grant) = invitation_with_lifetime(lifetime);
        fixture
            .repository
            .create_yard_guest_invite(
                &candidate,
                &candidate_grant,
                &event("created", &candidate, None, 1),
            )
            .expect("inclusive lifetime boundary");
    }
    for lifetime in [
        YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS - 1,
        YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS + 1,
    ] {
        let fixture = Fixture::new();
        let (candidate, candidate_grant) = invitation_with_lifetime(lifetime);
        assert_eq!(
            fixture.repository.create_yard_guest_invite(
                &candidate,
                &candidate_grant,
                &event("created", &candidate, None, 1),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn guest_invitation_rejects_ambiguous_email_and_nonproduction_scope() {
    let fixture = Fixture::new();
    let mut ambiguous = invitation();
    ambiguous.email = "a@b@c".to_owned();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &ambiguous,
            &grant(),
            &event("created", &ambiguous, None, 1),
        ),
        Err(RepositoryError::InvalidInput)
    );

    for mutation in [
        "UPDATE yard_environments SET kind = 'preview' WHERE id = 'environment_guest'",
        "UPDATE yard_environments SET status = 'deleted', deleted_at_ms = 2 WHERE id = 'environment_guest'",
    ] {
        let fixture = Fixture::new();
        fixture
            .repository
            .test_connection()
            .expect("connection")
            .execute(mutation, [])
            .expect("environment mutation");
        assert_eq!(
            fixture.repository.create_yard_guest_invite(
                &invitation(),
                &grant(),
                &event("created", &invitation(), None, 1),
            ),
            Err(RepositoryError::NotFound)
        );
    }

    let fixture = Fixture::new();
    let mut foreign = invitation();
    foreign.environment_id = Some("environment_foreign".to_owned());
    let mut foreign_grant = grant();
    foreign_grant.environment_id = foreign.environment_id.clone();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &foreign,
            &foreign_grant,
            &event("created", &foreign, None, 1),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn guest_creation_rejects_invalid_text_and_hash_before_mutation() {
    let fixture = Fixture::new();
    let mut invalid_text = invitation();
    invalid_text.id.clear();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &invalid_text,
            &grant(),
            &event("created", &invalid_text, None, 1),
        ),
        Err(RepositoryError::InvalidInput)
    );

    let mut invalid_hash = invitation();
    invalid_hash.token_hash = "bad".to_owned();
    assert_eq!(
        fixture.repository.create_yard_guest_invite(
            &invalid_hash,
            &grant(),
            &event("created", &invalid_hash, None, 1),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

fn invitation_with_lifetime(
    lifetime_ms: u64,
) -> (
    blobyard_contract::NewYardGuestInvite,
    blobyard_contract::NewYardAccessGrant,
) {
    let mut candidate = invitation();
    candidate.expires_at_ms = candidate.created_at_ms + lifetime_ms;
    let mut candidate_grant = grant();
    candidate_grant.expires_at_ms = Some(candidate.expires_at_ms);
    (candidate, candidate_grant)
}
