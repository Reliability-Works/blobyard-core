#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::api::AppState;
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardDeploy, NewYardGuestInvite,
    YardAccessPrincipalKind, YardGuestInviteRecord, YardStartRecord, yard_guest_audit_metadata,
};
use blobyard_core::Slug;

pub(super) fn start_yard(state: &AppState) -> YardStartRecord {
    let yard = NewWebYard {
        id: "yard_invitation".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("invitation").expect("yard name"),
        host_label: "invitation-fixture".to_owned(),
        created_at_ms: 1,
    };
    let deploy = NewYardDeploy {
        id: "yarddeploy_invitation".to_owned(),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "client-invitation".to_owned(),
        manifest_root: ".blobyard-yard/yard_invitation/client-invitation/".to_owned(),
        deployment_host_label: "invitation-deploy-fixture".to_owned(),
        spa: false,
        clean_urls: false,
        created_at_ms: 1,
    };
    state
        .repository
        .start_yard_deploy(
            &yard,
            &deploy,
            &blobyard_testkit::yard_event("yard.created", "web_yard", "yardId", &yard.id, 1),
        )
        .expect("yard")
}

pub(super) fn create_invitation(
    state: &AppState,
    yard: &blobyard_contract::WebYardRecord,
    raw_token: &str,
    expires_at_ms: u64,
) -> YardGuestInviteRecord {
    let invitation = NewYardGuestInvite {
        id: "ygi_cccccccccccccccccccccccccccccccc".to_owned(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        yard_id: yard.id.clone(),
        environment_id: None,
        email: "guest@example.test".to_owned(),
        token_hash: crate::auth::hash(raw_token),
        grant_id: "yardgrant_invitation".to_owned(),
        created_at_ms: 1,
        expires_at_ms,
    };
    let grant = NewYardAccessGrant {
        id: invitation.grant_id.clone(),
        yard_id: invitation.yard_id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: invitation.id.clone(),
        app_roles: Vec::new(),
        created_at_ms: invitation.created_at_ms,
        created_by_principal: "operator".to_owned(),
        expires_at_ms: Some(invitation.expires_at_ms),
    };
    let event = NewAuditEvent {
        id: "audit_invitation".to_owned(),
        workspace_id: invitation.workspace_id.clone(),
        actor: "operator".to_owned(),
        action: "yard.guest_invite.created".to_owned(),
        request_id: "request_invitation".to_owned(),
        target_type: "yard_guest_invite".to_owned(),
        metadata: yard_guest_audit_metadata(&invitation, None),
        created_at_ms: invitation.created_at_ms,
    };
    state
        .repository
        .create_yard_guest_invite(&invitation, &grant, &event)
        .expect("invitation")
}
