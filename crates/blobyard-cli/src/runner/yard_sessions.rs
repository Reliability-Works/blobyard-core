use super::yards::select_yard;
use super::{Runner, command_result, validate_resource_name};
use crate::config::validate_yard_name;
use crate::yard_commands::{RevokeYardSessionArgs, YardSessionsListArgs};
use blobyard_api_client::{
    ApiRequest, EmptyResponse, Endpoint, ListYardSessionsQuery, ListYardSessionsResponse,
    RevokeYardSessionRequest, YardSessionStatus, YardSessionSummary,
};
use blobyard_core::{BlobyardError, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardSessionsOutput<'a> {
    yard: &'a Slug,
    sessions: &'a [YardSessionSummary],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardSessionRevokeOutput<'a> {
    yard: &'a Slug,
    session_id: &'a str,
    revoked: bool,
}

impl Runner {
    pub(super) async fn list_yard_sessions(
        &self,
        arguments: &YardSessionsListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, arguments.name.as_deref())?;
        let request = ApiRequest::new(Endpoint::ListYardSessions).with_query(
            ListYardSessionsQuery {
                yard_id: selected.id.clone(),
            }
            .into_query(),
        );
        let success = self
            .execute_authed::<ListYardSessionsResponse>(request)
            .await?;
        command_result(
            &YardSessionsOutput {
                yard: &selected.name,
                sessions: &success.data().sessions,
            },
            session_lines(&success.data().sessions),
            success.request_id(),
        )
    }

    pub(super) async fn revoke_yard_session(
        &self,
        arguments: &RevokeYardSessionArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        validate_resource_name(&arguments.session_id, "Yard session")?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, Some(yard.as_str()))?;
        let request = self.mutation(Endpoint::RevokeYardSession).with_json(
            RevokeYardSessionRequest {
                session_id: arguments.session_id.clone(),
                yard_id: selected.id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        command_result(
            &YardSessionRevokeOutput {
                yard: &yard,
                session_id: &arguments.session_id,
                revoked: true,
            },
            format!("Revoked Yard session '{}'.", arguments.session_id),
            success.request_id(),
        )
    }
}

fn session_lines(sessions: &[YardSessionSummary]) -> String {
    if sessions.is_empty() {
        return "No Yard browser sessions found.".to_owned();
    }
    sessions
        .iter()
        .map(|session| {
            format!(
                "{}\t{}\t{}\t{}\tcreated {}\texpires {}\tlast used {}",
                session.id,
                status_label(session.status),
                session.user_display_name,
                session.host_label,
                session.created_at,
                session.expires_at,
                session.last_used_at.as_deref().unwrap_or("never")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const fn status_label(status: YardSessionStatus) -> &'static str {
    match status {
        YardSessionStatus::Active => "active",
        YardSessionStatus::Expired => "expired",
        YardSessionStatus::Revoked => "revoked",
    }
}

#[cfg(test)]
#[path = "yard_sessions_tests.rs"]
mod tests;
