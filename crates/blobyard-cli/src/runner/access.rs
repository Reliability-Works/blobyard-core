use super::yards::select_yard;
use super::{Runner, command_result};
use crate::config::validate_yard_name;
use crate::yard_commands::{
    AccessListArgs, GrantAccessArgs, RevokeAccessArgs, SetAccessRolesArgs, SetVisibilityArgs,
};
use blobyard_api_client::{
    ApiRequest, EmptyResponse, Endpoint, GetYardAccessQuery, GrantYardAccessRequest,
    RevokeYardAccessRequest, SetYardAccessRolesRequest, SetYardAccessRolesResponse,
    SetYardVisibilityRequest, YardAccessGrantResponse, YardAccessGrantSummary,
    YardAccessPrincipalKind, YardAccessResponse, YardVisibility, YardVisibilityResponse,
};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccessListOutput<'a> {
    yard: &'a Slug,
    visibility: YardVisibility,
    grants: &'a [YardAccessGrantSummary],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VisibilityOutput<'a> {
    yard: &'a Slug,
    visibility: YardVisibility,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrantOutput<'a> {
    yard: &'a Slug,
    grant: &'a YardAccessGrantSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevokeOutput<'a> {
    yard: &'a Slug,
    grant_id: &'a str,
    revoked: bool,
}

impl Runner {
    pub(super) async fn access_list(
        &self,
        arguments: &AccessListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, arguments.name.as_deref())?;
        let request = ApiRequest::new(Endpoint::GetYardAccess).with_query(
            GetYardAccessQuery {
                yard_id: selected.id.clone(),
            }
            .into_query(),
        );
        let success = self.execute_authed::<YardAccessResponse>(request).await?;
        let access = success.data();
        command_result(
            &AccessListOutput {
                yard: &selected.name,
                visibility: access.visibility,
                grants: &access.grants,
            },
            access_lines(access),
            success.request_id(),
        )
    }

    pub(super) async fn access_set_visibility(
        &self,
        arguments: &SetVisibilityArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        let visibility = parse_visibility(&arguments.visibility)?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, Some(yard.as_str()))?;
        let request = self.mutation(Endpoint::SetYardVisibility).with_json(
            SetYardVisibilityRequest {
                yard_id: selected.id.clone(),
                visibility,
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardVisibilityResponse>(request)
            .await?;
        let output = VisibilityOutput {
            yard: &yard,
            visibility: success.data().visibility,
        };
        command_result(
            &output,
            format!(
                "Set Web Yard '{yard}' visibility to {}.",
                visibility_label(success.data().visibility)
            ),
            success.request_id(),
        )
    }

    pub(super) async fn access_grant(
        &self,
        arguments: &GrantAccessArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        let principal_kind = parse_principal_kind(&arguments.principal_kind)?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, Some(yard.as_str()))?;
        let request = self.mutation(Endpoint::GrantYardAccess).with_json(
            GrantYardAccessRequest {
                yard_id: selected.id.clone(),
                principal_kind,
                principal_id: arguments.principal_id.clone(),
                app_roles: arguments.roles.clone(),
                environment_id: arguments.environment.clone(),
                expires_at: arguments.expires.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardAccessGrantResponse>(request)
            .await?;
        let grant = &success.data().grant;
        command_result(
            &GrantOutput { yard: &yard, grant },
            grant_line(grant),
            success.request_id(),
        )
    }

    pub(super) async fn access_revoke(
        &self,
        arguments: &RevokeAccessArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::RevokeYardAccess).with_json(
            RevokeYardAccessRequest {
                yard_id: selected.id.clone(),
                grant_id: arguments.grant_id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        let output = RevokeOutput {
            yard: &yard,
            grant_id: &arguments.grant_id,
            revoked: true,
        };
        command_result(
            &output,
            format!("Revoked access grant '{}'.", arguments.grant_id),
            success.request_id(),
        )
    }

    pub(super) async fn access_set_roles(
        &self,
        arguments: &SetAccessRolesArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::SetYardAccessRoles).with_json(
            SetYardAccessRolesRequest {
                yard_id: selected.id.clone(),
                grant_id: arguments.grant_id.clone(),
                app_roles: arguments.roles.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<SetYardAccessRolesResponse>(request)
            .await?;
        command_result(
            &GrantOutput {
                yard: &yard,
                grant: &success.data().grant,
            },
            grant_line(&success.data().grant),
            success.request_id(),
        )
    }
}

fn access_lines(access: &YardAccessResponse) -> String {
    let mut lines = vec![format!(
        "visibility\t{}",
        visibility_label(access.visibility)
    )];
    if access.grants.is_empty() {
        lines.push("No active grants.".to_owned());
    } else {
        lines.extend(access.grants.iter().map(grant_line));
    }
    lines.join("\n")
}

fn grant_line(grant: &YardAccessGrantSummary) -> String {
    format!(
        "{}\t{}\t{}\troles {}\texpires {}\t{}",
        principal_kind_label(grant.principal_kind),
        grant.principal_id,
        grant
            .environment_id
            .as_deref()
            .unwrap_or("all-environments"),
        if grant.app_roles.is_empty() {
            "none".to_owned()
        } else {
            grant.app_roles.join(",")
        },
        grant.expires_at.as_deref().unwrap_or("never"),
        grant.id
    )
}

fn parse_visibility(value: &str) -> Result<YardVisibility, BlobyardError> {
    match value {
        "public" => Ok(YardVisibility::Public),
        "owner" => Ok(YardVisibility::Owner),
        "selected" => Ok(YardVisibility::Selected),
        "workspace" => Ok(YardVisibility::Workspace),
        "authenticated-link" => Ok(YardVisibility::AuthenticatedLink),
        "any-authenticated" => Ok(YardVisibility::AnyAuthenticated),
        _ => Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Visibility must be public, owner, selected, workspace, authenticated-link, or any-authenticated.",
        )),
    }
}

fn parse_principal_kind(value: &str) -> Result<YardAccessPrincipalKind, BlobyardError> {
    match value {
        "user" => Ok(YardAccessPrincipalKind::User),
        "group" => Ok(YardAccessPrincipalKind::Group),
        "guest-invite" => Ok(YardAccessPrincipalKind::GuestInvite),
        "link" => Ok(YardAccessPrincipalKind::Link),
        _ => Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Principal kind must be user, group, guest-invite, or link.",
        )),
    }
}

const fn visibility_label(visibility: YardVisibility) -> &'static str {
    match visibility {
        YardVisibility::Public => "public",
        YardVisibility::Owner => "owner",
        YardVisibility::Selected => "selected",
        YardVisibility::Workspace => "workspace",
        YardVisibility::AuthenticatedLink => "authenticated-link",
        YardVisibility::AnyAuthenticated => "any-authenticated",
    }
}

const fn principal_kind_label(kind: YardAccessPrincipalKind) -> &'static str {
    match kind {
        YardAccessPrincipalKind::User => "user",
        YardAccessPrincipalKind::Group => "group",
        YardAccessPrincipalKind::GuestInvite => "guest-invite",
        YardAccessPrincipalKind::Link => "link",
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
