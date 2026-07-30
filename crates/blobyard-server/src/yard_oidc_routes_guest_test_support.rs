#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YardAccessPrincipalKind, YardGuestLoginKeyRecord,
    YardSubjectKind, YardSubjectRecord, yard_guest_audit_metadata,
};

pub(super) fn seed_guest(state: &crate::api::AppState, yard: &NewWebYard) {
    let environment = state
        .repository
        .list_yard_environments(&yard.id)
        .expect("environments")
        .pop()
        .expect("production environment");
    let invitation = guest_invitation(yard, &environment.id);
    let grant = NewYardAccessGrant {
        id: invitation.grant_id.clone(),
        yard_id: yard.id.clone(),
        environment_id: invitation.environment_id.clone(),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: invitation.id.clone(),
        app_roles: Vec::new(),
        created_at_ms: 5,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: Some(invitation.expires_at_ms),
    };
    state
        .repository
        .create_yard_guest_invite(
            &invitation,
            &grant,
            &guest_event("created", &invitation, None, 5),
        )
        .expect("guest invitation");
    let subject = YardSubjectRecord {
        id: "guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        kind: YardSubjectKind::Guest,
        workspace_id: yard.workspace_id.clone(),
        local_user_id: None,
        invitation_id: Some(invitation.id.clone()),
        created_at_ms: 6,
        revoked_at_ms: None,
    };
    state
        .repository
        .accept_yard_guest_invite(
            &invitation.token_hash,
            &subject,
            &guest_key(&invitation, &subject),
            &guest_continuation(yard, &environment.id, &subject),
            &guest_event("accepted", &invitation, Some(&subject.id), 6),
            6,
        )
        .expect("accepted guest");
}

fn guest_invitation(yard: &NewWebYard, environment_id: &str) -> NewYardGuestInvite {
    NewYardGuestInvite {
        id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        yard_id: yard.id.clone(),
        environment_id: Some(environment_id.to_owned()),
        email: "guest@example.test".to_owned(),
        token_hash: crate::auth::hash("guest-invitation-token"),
        grant_id: "yardgrant_oidc_guest".to_owned(),
        created_at_ms: 5,
        expires_at_ms: 600_005,
    }
}

fn guest_key(
    invitation: &NewYardGuestInvite,
    subject: &YardSubjectRecord,
) -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: "yardguestkey_oidc_fixture".to_owned(),
        subject_id: subject.id.clone(),
        invitation_id: invitation.id.clone(),
        workspace_id: invitation.workspace_id.clone(),
        token_prefix: "byg_oidc".to_owned(),
        secret_hash: crate::auth::hash("guest-login-key"),
        created_at_ms: 6,
        expires_at_ms: invitation.expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn guest_continuation(
    yard: &NewWebYard,
    environment_id: &str,
    subject: &YardSubjectRecord,
) -> NewYardContinuation {
    NewYardContinuation {
        id: "yardcont_oidc_guest".to_owned(),
        continuation_hash: crate::auth::hash("guest-continuation"),
        code_hash: crate::auth::hash("guest-code"),
        yard_id: yard.id.clone(),
        environment_id: environment_id.to_owned(),
        host_label: yard.host_label.clone(),
        user_id: subject.id.clone(),
        return_path: "/reports".to_owned(),
        created_at_ms: 6,
        expires_at_ms: 6 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn guest_event(
    action: &str,
    invitation: &NewYardGuestInvite,
    subject_id: Option<&str>,
    at: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_oidc_guest_{action}"),
        workspace_id: invitation.workspace_id.clone(),
        actor: "fixture".to_owned(),
        action: format!("yard.guest_invite.{action}"),
        request_id: format!("request_oidc_guest_{action}"),
        target_type: "yard_guest_invite".to_owned(),
        metadata: yard_guest_audit_metadata(invitation, subject_id),
        created_at_ms: at,
    }
}
