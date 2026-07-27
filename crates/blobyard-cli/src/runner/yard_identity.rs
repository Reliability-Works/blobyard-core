use super::{Runner, command_result};
use crate::yard_commands::{
    ApplicationPolicyGetArgs, ApplicationPolicySetArgs, ManagementRolesListArgs,
    RevokeManagementRoleArgs, SetManagementRoleArgs,
};
use blobyard_api_client::{
    ApiRequest, EmptyResponse, Endpoint, GetYardApplicationPolicyQuery,
    ListYardManagementRolesQuery, ListYardManagementRolesResponse, RevokeYardManagementRoleRequest,
    SetYardApplicationPolicyRequest, SetYardManagementRoleRequest, YardApplicationPolicyResponse,
    YardManagementRole, YardManagementRoleAssignment,
};
use blobyard_core::{ApplicationPolicyGraph, BlobyardError, ErrorCode, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagementRolesOutput<'a> {
    yard: &'a Slug,
    items: &'a [YardManagementRoleAssignment],
    next_cursor: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagementRoleMutationOutput<'a> {
    yard: &'a Slug,
    assignment: Option<&'a YardManagementRoleAssignment>,
    user_id: &'a str,
    revoked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationPolicyOutput<'a> {
    yard: &'a Slug,
    response: &'a YardApplicationPolicyResponse,
}

impl Runner {
    pub(super) async fn list_management_roles(
        &self,
        arguments: &ManagementRolesListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = ApiRequest::new(Endpoint::ListYardManagementRoles).with_query(
            ListYardManagementRolesQuery {
                yard_id: selected.id.clone(),
                cursor: arguments.cursor.clone(),
            }
            .into_query(),
        );
        let success = self
            .execute_authed::<ListYardManagementRolesResponse>(request)
            .await?;
        command_result(
            &ManagementRolesOutput {
                yard: &yard,
                items: &success.data().items,
                next_cursor: success.data().next_cursor.as_deref(),
            },
            role_lines(success.data()),
            success.request_id(),
        )
    }

    pub(super) async fn set_management_role(
        &self,
        arguments: &SetManagementRoleArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let role = parse_role(&arguments.role)?;
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::SetYardManagementRole).with_json(
            SetYardManagementRoleRequest {
                yard_id: selected.id.clone(),
                user_id: arguments.user_id.clone(),
                role,
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardManagementRoleAssignment>(request)
            .await?;
        command_result(
            &ManagementRoleMutationOutput {
                yard: &yard,
                assignment: Some(success.data()),
                user_id: &arguments.user_id,
                revoked: false,
            },
            format!(
                "Set '{}' to {} on Web Yard '{yard}'.",
                arguments.user_id,
                role_label(success.data().role)
            ),
            success.request_id(),
        )
    }

    pub(super) async fn revoke_management_role(
        &self,
        arguments: &RevokeManagementRoleArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::RevokeYardManagementRole).with_json(
            RevokeYardManagementRoleRequest {
                yard_id: selected.id.clone(),
                user_id: arguments.user_id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        command_result(
            &ManagementRoleMutationOutput {
                yard: &yard,
                assignment: None,
                user_id: &arguments.user_id,
                revoked: true,
            },
            format!("Revoked '{}' from Web Yard '{yard}'.", arguments.user_id),
            success.request_id(),
        )
    }

    pub(super) async fn get_application_policy(
        &self,
        arguments: &ApplicationPolicyGetArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = ApiRequest::new(Endpoint::GetYardApplicationPolicy).with_query(
            GetYardApplicationPolicyQuery {
                yard_id: selected.id.clone(),
            }
            .into_query(),
        );
        let success = self
            .execute_authed::<YardApplicationPolicyResponse>(request)
            .await?;
        let output = ApplicationPolicyOutput {
            yard: &yard,
            response: success.data(),
        };
        let line = policy_line(success.data());
        command_result(&output, line, success.request_id())
    }

    pub(super) async fn set_application_policy(
        &self,
        arguments: &ApplicationPolicySetArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let policy = read_policy(arguments)?;
        let (yard, selected) = self.selected_named_yard(&arguments.name).await?;
        let request = self.mutation(Endpoint::SetYardApplicationPolicy).with_json(
            SetYardApplicationPolicyRequest {
                yard_id: selected.id.clone(),
                source_manifest_digest: arguments.source_manifest_digest.clone(),
                policy,
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardApplicationPolicyResponse>(request)
            .await?;
        command_result(
            &ApplicationPolicyOutput {
                yard: &yard,
                response: success.data(),
            },
            policy_line(success.data()),
            success.request_id(),
        )
    }
}

fn role_lines(response: &ListYardManagementRolesResponse) -> String {
    if response.items.is_empty() {
        return "No Yard management roles.".to_owned();
    }
    response
        .items
        .iter()
        .map(|item| format!("{}\t{}", role_label(item.role), item.user_id))
        .collect::<Vec<_>>()
        .join("\n")
}

fn policy_line(response: &YardApplicationPolicyResponse) -> String {
    response.policy.as_ref().map_or_else(
        || "No approved application policy.".to_owned(),
        |policy| {
            format!(
                "Application policy revision {} with {} roles.",
                policy.revision,
                policy.graph.roles.len()
            )
        },
    )
}

fn parse_role(value: &str) -> Result<YardManagementRole, BlobyardError> {
    match value {
        "owner" => Ok(YardManagementRole::Owner),
        "admin" => Ok(YardManagementRole::Admin),
        "developer" => Ok(YardManagementRole::Developer),
        "auditor" => Ok(YardManagementRole::Auditor),
        _ => Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Management role must be owner, admin, developer, or auditor.",
        )),
    }
}

const fn role_label(role: YardManagementRole) -> &'static str {
    match role {
        YardManagementRole::Owner => "owner",
        YardManagementRole::Admin => "admin",
        YardManagementRole::Developer => "developer",
        YardManagementRole::Auditor => "auditor",
    }
}

fn read_policy(
    arguments: &ApplicationPolicySetArgs,
) -> Result<ApplicationPolicyGraph, BlobyardError> {
    if let Some(value) = &arguments.policy_json {
        return decode_policy(value.as_bytes());
    }
    let path = arguments.policy.as_deref().ok_or_else(|| {
        BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Supply either --policy or --policy-json.",
        )
    })?;
    let bytes = std::fs::read(path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::NotFound
        } else {
            ErrorCode::InternalError
        };
        BlobyardError::new(code, format!("Could not read {}.", path.display()))
    })?;
    decode_policy(&bytes)
}

fn decode_policy(bytes: &[u8]) -> Result<ApplicationPolicyGraph, BlobyardError> {
    serde_json::from_slice(bytes).map_err(|_error| {
        BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Policy file must be JSON containing only defaultRole and roles.",
        )
    })
}

#[cfg(test)]
#[path = "yard_identity_tests.rs"]
mod tests;
