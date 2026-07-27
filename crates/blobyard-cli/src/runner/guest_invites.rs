use super::yards::select_yard;
use super::{Runner, command_result, validate_resource_name};
use crate::yard_commands::{CreateGuestInviteArgs, GuestInvitesListArgs, RevokeGuestInviteArgs};
use blobyard_api_client::{
    ApiRequest, CreateYardGuestInviteRequest, CreateYardGuestInviteResponse, Endpoint,
    ListYardGuestInvitesQuery, ListYardGuestInvitesResponse, RevokeYardGuestInviteRequest,
    RevokeYardGuestInviteResponse, YardGuestInvite, YardGuestInviteStatus,
};
use blobyard_core::{BlobyardError, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuestInviteListOutput<'a> {
    yard: &'a Slug,
    items: &'a [YardGuestInvite],
    next_cursor: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuestInviteCreateOutput<'a> {
    yard: &'a Slug,
    invitation: &'a YardGuestInvite,
    invitation_url: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuestInviteRevokeOutput<'a> {
    yard: &'a Slug,
    invitation: &'a YardGuestInvite,
}

impl Runner {
    pub(super) async fn list_guest_invites(
        &self,
        arguments: &GuestInvitesListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, arguments.name.as_deref())?;
        let request = ApiRequest::new(Endpoint::ListYardGuestInvites).with_query(
            ListYardGuestInvitesQuery {
                yard_id: selected.id.clone(),
                cursor: arguments.cursor.clone(),
                limit: Some(50),
            }
            .into_query(),
        );
        let success = self
            .execute_authed::<ListYardGuestInvitesResponse>(request)
            .await?;
        command_result(
            &GuestInviteListOutput {
                yard: &selected.name,
                items: &success.data().items,
                next_cursor: success.data().next_cursor.as_deref(),
            },
            invitation_lines(&success.data().items, success.data().next_cursor.as_deref()),
            success.request_id(),
        )
    }

    pub(super) async fn create_guest_invite(
        &self,
        arguments: &CreateGuestInviteArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::CreateYardGuestInvite).with_json(
            CreateYardGuestInviteRequest {
                yard_id: selected.id,
                environment_id: arguments.environment.clone(),
                email: arguments.email.clone(),
                app_roles: arguments.roles.clone(),
                expires_at: arguments.expires.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<CreateYardGuestInviteResponse>(request)
            .await?;
        command_result(
            &GuestInviteCreateOutput {
                yard: &yard,
                invitation: &success.data().invitation,
                invitation_url: &success.data().invitation_url,
            },
            format!(
                "{}\nInvitation URL (shown once): {}",
                invitation_line(&success.data().invitation),
                success.data().invitation_url
            ),
            success.request_id(),
        )
    }

    pub(super) async fn revoke_guest_invite(
        &self,
        arguments: &RevokeGuestInviteArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_resource_name(&arguments.invitation_id, "Yard guest invitation")?;
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::RevokeYardGuestInvite).with_json(
            RevokeYardGuestInviteRequest {
                yard_id: selected.id,
                invitation_id: arguments.invitation_id.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<RevokeYardGuestInviteResponse>(request)
            .await?;
        command_result(
            &GuestInviteRevokeOutput {
                yard: &yard,
                invitation: &success.data().invitation,
            },
            invitation_line(&success.data().invitation),
            success.request_id(),
        )
    }
}

fn invitation_lines(items: &[YardGuestInvite], next_cursor: Option<&str>) -> String {
    if items.is_empty() {
        return "No Yard guest invitations found.".to_owned();
    }
    let mut lines = items.iter().map(invitation_line).collect::<Vec<_>>();
    if let Some(cursor) = next_cursor {
        lines.push(format!("Next cursor: {cursor}"));
    }
    lines.join("\n")
}

fn invitation_line(invitation: &YardGuestInvite) -> String {
    format!(
        "{}\t{}\t{}\t{}\troles {}\texpires {}",
        invitation.id,
        status_label(invitation.status),
        invitation.email,
        invitation
            .environment_id
            .as_deref()
            .unwrap_or("all-environments"),
        if invitation.app_roles.is_empty() {
            "none".to_owned()
        } else {
            invitation.app_roles.join(",")
        },
        invitation.expires_at,
    )
}

const fn status_label(status: YardGuestInviteStatus) -> &'static str {
    match status {
        YardGuestInviteStatus::Pending => "pending",
        YardGuestInviteStatus::Accepted => "accepted",
        YardGuestInviteStatus::Revoked => "revoked",
    }
}

#[cfg(test)]
#[path = "guest_invites_tests.rs"]
mod tests;
