use super::{Runner, command_result};
use crate::commands::{CreateInboxArgs, RevokeInboxArgs, SetRetentionArgs, ShareArgs, UploadArgs};
use crate::headless_commands::{RevokeShareArgs, SharesCommand};
use blobyard_api_client::{
    ApiRequest, ClearRetentionResponse, CreateInboxRequest, CreateInboxResponse,
    CreateShareRequest, CreateShareResponse, EmptyResponse, Endpoint, InboxPage, ListInboxesQuery,
    ListSharesQuery, RetentionPolicy, RetentionQuery, RevokeInboxRequest, RevokeShareRequest,
    SetRetentionRequest, SharePage,
};
use blobyard_core::{BlobyardError, BlobyardUri, ErrorCode};
use std::path::PathBuf;
use std::str::FromStr;

impl Runner {
    pub(super) async fn execute_shares(
        &self,
        command: &SharesCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            SharesCommand::List => self.list_shares().await,
            SharesCommand::Revoke(arguments) => self.revoke_share(arguments).await,
        }
    }

    pub(super) async fn create_share(
        &self,
        arguments: &ShareArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_duration(arguments.expires.as_deref())?;
        validate_email(arguments.notify.as_deref())?;
        let target = self.resolve_share_target(&arguments.target).await?;
        let request = self.mutation(Endpoint::CreateShare).with_json(
            CreateShareRequest {
                target,
                expires: arguments.expires.clone(),
                notify: arguments.notify.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<CreateShareResponse>(request).await?;
        let human = format!("Share URL: {}", success.data().share_url.expose_secret());
        command_result(success.data(), human, success.request_id())
    }

    async fn resolve_share_target(&self, value: &str) -> Result<BlobyardUri, BlobyardError> {
        if let Ok(uri) = BlobyardUri::from_str(value) {
            return Ok(uri);
        }
        let arguments = local_share_arguments(value)?;
        let (completed, _request_id) = self.upload_files(&arguments).await?;
        completed_share_target(completed)
    }

    pub(super) async fn create_inbox(
        &self,
        arguments: &CreateInboxArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_name(&arguments.name)?;
        validate_duration(arguments.expires.as_deref())?;
        let (workspace, project) = self.scope()?;
        let request = self.mutation(Endpoint::CreateInbox).with_json(
            CreateInboxRequest {
                workspace,
                project,
                name: arguments.name.clone(),
                expires: arguments.expires.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<CreateInboxResponse>(request).await?;
        let human = success.data().inbox_url.expose_secret().to_owned();
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn list_shares(&self) -> Result<crate::CommandResult, BlobyardError> {
        let workspace = self.config.workspace().cloned().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a workspace with --workspace or configuration.",
            )
        })?;
        let request = ApiRequest::new(Endpoint::ListShares)
            .with_query(ListSharesQuery { workspace }.into_query());
        let success = self.execute_authed::<SharePage>(request).await?;
        let human = share_lines(success.data());
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn revoke_share(
        &self,
        arguments: &RevokeShareArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_identifier(&arguments.share_id, "share")?;
        let request = self.mutation(Endpoint::RevokeShare).with_json(
            RevokeShareRequest {
                share_id: arguments.share_id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        command_result(success.data(), "Share revoked.", success.request_id())
    }

    pub(super) async fn list_inboxes(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = ApiRequest::new(Endpoint::ListInboxes).with_query(
            ListInboxesQuery {
                workspace,
                project,
                cursor: None,
            }
            .into_query(),
        );
        let success = self.execute_authed::<InboxPage>(request).await?;
        let human = inbox_lines(success.data());
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn revoke_inbox(
        &self,
        arguments: &RevokeInboxArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_identifier(&arguments.inbox_id, "inbox")?;
        let request = self.mutation(Endpoint::RevokeInbox).with_json(
            RevokeInboxRequest {
                inbox_id: arguments.inbox_id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        command_result(success.data(), "Inbox revoked.", success.request_id())
    }

    pub(super) async fn show_retention(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = ApiRequest::new(Endpoint::GetRetention)
            .with_query(RetentionQuery { workspace, project }.into_query());
        let success = self.execute_authed::<RetentionPolicy>(request).await?;
        retention_result(&success)
    }

    pub(super) async fn retention_overview(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = ApiRequest::new(Endpoint::GetRetentionOverview)
            .with_query(RetentionQuery { workspace, project }.into_query());
        self.json_read(request).await
    }

    pub(super) async fn set_retention(
        &self,
        arguments: &SetRetentionArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_glob(arguments.branch.as_deref())?;
        validate_glob(arguments.path.as_deref())?;
        let (workspace, project) = self.scope()?;
        let policy = RetentionPolicy {
            keep_latest: arguments.latest,
            branch_glob: arguments.branch.clone(),
            path_glob: arguments.path.clone(),
        };
        let request = self.mutation(Endpoint::SetRetention).with_json(
            SetRetentionRequest {
                workspace,
                project,
                policy,
            }
            .into_json(),
        );
        let success = self.execute_authed::<RetentionPolicy>(request).await?;
        retention_result(&success)
    }

    pub(super) async fn clear_retention(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = self
            .mutation(Endpoint::ClearRetention)
            .with_query(RetentionQuery { workspace, project }.into_query());
        let success = self
            .execute_authed::<ClearRetentionResponse>(request)
            .await?;
        command_result(
            success.data(),
            "Retention policy cleared.",
            success.request_id(),
        )
    }
}

fn retention_result(
    success: &blobyard_api_client::ApiSuccess<RetentionPolicy>,
) -> Result<crate::CommandResult, BlobyardError> {
    let human = retention_line(success.data());
    command_result(success.data(), human, success.request_id())
}

fn inbox_lines(page: &InboxPage) -> String {
    if page.items().is_empty() {
        return "No inboxes found.".to_owned();
    }
    page.items()
        .iter()
        .map(|inbox| format!("{}\t{}\t{}", inbox.id, inbox.name, inbox.expires_at))
        .collect::<Vec<_>>()
        .join("\n")
}

fn share_lines(page: &SharePage) -> String {
    if page.items().is_empty() {
        return "No shares found.".to_owned();
    }
    page.items()
        .iter()
        .map(|share| format!("{}\t{}\t{}", share.id, share.status, share.expires_at))
        .collect::<Vec<_>>()
        .join("\n")
}

fn retention_line(policy: &RetentionPolicy) -> String {
    format!(
        "Keep latest {} (branch: {}, path: {}).",
        policy.keep_latest,
        policy.branch_glob.as_deref().map_or("any", |value| value),
        policy.path_glob.as_deref().map_or("any", |value| value)
    )
}

pub(super) fn validate_duration(value: Option<&str>) -> Result<(), BlobyardError> {
    let valid = value.is_none_or(|duration| {
        let split = duration.len().saturating_sub(1);
        let (amount, unit) = duration.split_at(split);
        amount.parse::<u64>().is_ok_and(|number| number > 0)
            && matches!(unit, "s" | "m" | "h" | "d")
    });
    validation_result(
        valid,
        "Duration must be a positive value such as 24h or 7d.",
    )
}

fn local_share_arguments(value: &str) -> Result<UploadArgs, BlobyardError> {
    let source = PathBuf::from(value);
    let regular_file = !value.starts_with("blobyard:")
        && std::fs::symlink_metadata(&source).is_ok_and(|metadata| metadata.file_type().is_file());
    if !regular_file {
        return Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "The share target must be a regular local file or valid blobyard:// URI.",
        ));
    }
    Ok(UploadArgs {
        source,
        path: None,
        include_ignored: false,
    })
}

fn completed_share_target(
    completed: Vec<blobyard_api_client::CompleteUploadResponse>,
) -> Result<BlobyardUri, BlobyardError> {
    completed
        .into_iter()
        .next()
        .map(|object| object.uri)
        .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))
}

fn validate_name(value: &str) -> Result<(), BlobyardError> {
    let valid =
        !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control);
    validation_result(valid, "Inbox name must be 1-128 printable characters.")
}

fn validate_email(value: Option<&str>) -> Result<(), BlobyardError> {
    let valid = value.is_none_or(|email| {
        email.len() <= 320
            && !email.chars().any(char::is_control)
            && email.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && !domain.is_empty() && !domain.contains('@')
            })
    });
    validation_result(valid, "Notification email must be a valid address.")
}

pub(super) fn validate_identifier(value: &str, resource: &str) -> Result<(), BlobyardError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            format!("The {resource} identifier isn't valid."),
        ))
    }
}

fn validate_glob(value: Option<&str>) -> Result<(), BlobyardError> {
    let valid = value.is_none_or(|glob| {
        !glob.is_empty() && glob.len() <= 256 && !glob.chars().any(char::is_control)
    });
    validation_result(valid, "Retention globs must be 1-256 printable characters.")
}

fn validation_result(valid: bool, message: &'static str) -> Result<(), BlobyardError> {
    if valid {
        Ok(())
    } else {
        Err(BlobyardError::new(ErrorCode::InvalidRequest, message))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test assertions must fail loudly")]

    #[test]
    fn missing_completed_local_share_target_fails_closed() {
        let error = super::completed_share_target(Vec::new()).expect_err("missing completion");
        assert_eq!(error.code(), blobyard_core::ErrorCode::InternalError);
    }
}
