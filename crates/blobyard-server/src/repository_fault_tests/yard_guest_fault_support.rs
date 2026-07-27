use super::{FaultingRepository, Repository, event};
use blobyard_contract::{
    NewAuditEvent, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite, RepositoryError,
    YardAccessPrincipalKind, YardGuestLoginKeyRecord, YardGuestRepository, YardSubjectKind,
    YardSubjectRecord,
};
use std::sync::Arc;

pub(super) fn assert_guest_faults(inner: &Arc<dyn Repository>) {
    let records = guest_fault_records();
    let event = event(
        "yard.guest_invite.created",
        "yard_guest_invite",
        "invitationId",
        &records.invitation.id,
    );
    assert_guest_operation_faults(inner, &records, &event);
}

struct GuestFaultRecords {
    invitation: NewYardGuestInvite,
    grant: NewYardAccessGrant,
    subject: YardSubjectRecord,
    key: YardGuestLoginKeyRecord,
    continuation: NewYardContinuation,
}

fn guest_fault_records() -> GuestFaultRecords {
    let invitation = NewYardGuestInvite {
        id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        environment_id: None,
        email: "guest@example.test".to_owned(),
        token_hash: "a".repeat(64),
        grant_id: "yardgrant_fixture".to_owned(),
        created_at_ms: 1,
        expires_at_ms: 300_001,
    };
    let grant = guest_fault_grant(&invitation);
    let subject = guest_fault_subject(&invitation);
    let key = guest_fault_key(&invitation, &subject);
    let continuation = guest_fault_continuation(&invitation, &subject);
    GuestFaultRecords {
        invitation,
        grant,
        subject,
        key,
        continuation,
    }
}

fn guest_fault_grant(invitation: &NewYardGuestInvite) -> NewYardAccessGrant {
    NewYardAccessGrant {
        principal_id: invitation.id.clone(),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        app_roles: Vec::new(),
        environment_id: None,
        yard_id: invitation.yard_id.clone(),
        id: invitation.grant_id.clone(),
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: Some(300_001),
        created_at_ms: 1,
    }
}

fn guest_fault_subject(invitation: &NewYardGuestInvite) -> YardSubjectRecord {
    YardSubjectRecord {
        id: "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        kind: YardSubjectKind::Guest,
        workspace_id: invitation.workspace_id.clone(),
        local_user_id: None,
        invitation_id: Some(invitation.id.clone()),
        created_at_ms: 2,
        revoked_at_ms: None,
    }
}

fn guest_fault_key(
    invitation: &NewYardGuestInvite,
    subject: &YardSubjectRecord,
) -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: "ygk_cccccccccccccccccccccccccccccccc".to_owned(),
        subject_id: subject.id.clone(),
        invitation_id: invitation.id.clone(),
        workspace_id: invitation.workspace_id.clone(),
        token_prefix: "byg_fixture".to_owned(),
        secret_hash: "b".repeat(64),
        created_at_ms: 2,
        expires_at_ms: 300_001,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn guest_fault_continuation(
    invitation: &NewYardGuestInvite,
    subject: &YardSubjectRecord,
) -> NewYardContinuation {
    NewYardContinuation {
        id: "yardcont_fixture".to_owned(),
        continuation_hash: "c".repeat(64),
        code_hash: "d".repeat(64),
        yard_id: invitation.yard_id.clone(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "fixture-host".to_owned(),
        user_id: subject.id.clone(),
        return_path: "/".to_owned(),
        created_at_ms: 2,
        expires_at_ms: 300_002,
    }
}

fn assert_guest_operation_faults(
    inner: &Arc<dyn Repository>,
    records: &GuestFaultRecords,
    event: &NewAuditEvent,
) {
    let fail = || FaultingRepository::new(Arc::clone(inner), 0);
    assert_eq!(
        fail().list_yard_guest_invites(&records.invitation.yard_id, None, 50),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().yard_guest_invite_by_id(&records.invitation.id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().create_yard_guest_invite(&records.invitation, &records.grant, event),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().pending_yard_guest_invite_by_token(&records.invitation.token_hash, 2),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().accept_yard_guest_invite(
            &records.invitation.token_hash,
            &records.subject,
            &records.key,
            &records.continuation,
            event,
            2,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().revoke_yard_guest_invite(
            &records.invitation.yard_id,
            &records.invitation.id,
            2,
            event,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().authenticate_yard_guest_key(&records.key.secret_hash, 2),
        Err(RepositoryError::Unavailable)
    );
}
