use crate::hash;
use blobyard_contract::{
    NewAuditEvent, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS, YardAccessPrincipalKind,
    YardGuestLoginKeyRecord, YardSubjectKind, YardSubjectRecord, yard_guest_audit_metadata,
};

pub(super) const INVITATION_ID: &str = "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(super) const SUBJECT_ID: &str = "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub(super) const TOKEN_HASH: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
pub(super) const KEY_HASH: &str =
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
pub(super) const CREATED_AT_MS: u64 = 4_000_000_100;

pub(super) fn invitation(yard_id: &str, environment_id: &str) -> NewYardGuestInvite {
    NewYardGuestInvite {
        id: INVITATION_ID.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: Some(environment_id.to_owned()),
        email: "guest@example.test".to_owned(),
        token_hash: TOKEN_HASH.to_owned(),
        grant_id: "yardgrant_guest_fixture".to_owned(),
        created_at_ms: CREATED_AT_MS,
        expires_at_ms: CREATED_AT_MS + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS,
    }
}

pub(super) fn grant(invitation: &NewYardGuestInvite) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: invitation.grant_id.clone(),
        yard_id: invitation.yard_id.clone(),
        environment_id: invitation.environment_id.clone(),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: invitation.id.clone(),
        app_roles: vec!["editor".to_owned()],
        created_at_ms: invitation.created_at_ms,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: Some(invitation.expires_at_ms),
    }
}

pub(super) fn subject() -> YardSubjectRecord {
    YardSubjectRecord {
        id: SUBJECT_ID.to_owned(),
        kind: YardSubjectKind::Guest,
        workspace_id: "workspace_fixture".to_owned(),
        local_user_id: None,
        invitation_id: Some(INVITATION_ID.to_owned()),
        created_at_ms: CREATED_AT_MS + 1,
        revoked_at_ms: None,
    }
}

pub(super) fn key(expires_at_ms: u64) -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: "yardguestkey_fixture".to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        invitation_id: INVITATION_ID.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        token_prefix: "byg_fixture".to_owned(),
        secret_hash: KEY_HASH.to_owned(),
        created_at_ms: CREATED_AT_MS + 1,
        expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

pub(super) fn continuation(
    yard_id: &str,
    environment_id: &str,
    host_label: &str,
) -> NewYardContinuation {
    NewYardContinuation {
        id: "yardcont_guest_fixture".to_owned(),
        continuation_hash: hash('e'),
        code_hash: hash('f'),
        yard_id: yard_id.to_owned(),
        environment_id: environment_id.to_owned(),
        host_label: host_label.to_owned(),
        user_id: SUBJECT_ID.to_owned(),
        return_path: "/guest".to_owned(),
        created_at_ms: CREATED_AT_MS + 1,
        expires_at_ms: CREATED_AT_MS + 1 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

pub(super) fn event(
    action: &str,
    invitation: &NewYardGuestInvite,
    subject_id: Option<&str>,
    at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_guest_{action}_{at_ms}"),
        workspace_id: invitation.workspace_id.clone(),
        actor: "fixture".to_owned(),
        action: format!("yard.guest_invite.{action}"),
        request_id: format!("request_guest_{action}_{at_ms}"),
        target_type: "yard_guest_invite".to_owned(),
        metadata: yard_guest_audit_metadata(invitation, subject_id),
        created_at_ms: at_ms,
    }
}
