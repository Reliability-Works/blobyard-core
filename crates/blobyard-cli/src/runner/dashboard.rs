use super::{Runner, command_result};
use crate::account_commands::{
    AccountCommand, AccountDeleteCommand, AccountExportCommand, AccountExportDownloadArgs,
    CompleteAccountDeletionArgs, RetryAccountDeletionArgs,
};
use crate::billing_commands::{
    BillingCheckoutArgs, BillingCommand, BillingStorageCommand, StorageBillingArgs,
};
use crate::commands::Command;
use blobyard_api_client::{ApiRequest, Endpoint};
use blobyard_core::{BlobyardError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BillingSessionResult {
    url: String,
}

impl Runner {
    pub(super) async fn execute_dashboard_command(
        &self,
        command: &Command,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            Command::Billing { command } => self.execute_billing(command).await,
            Command::Account { command } => self.execute_account(command).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    async fn execute_billing(
        &self,
        command: &BillingCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            BillingCommand::Show => self.json_read(ApiRequest::new(Endpoint::GetBilling)).await,
            BillingCommand::Checkout(arguments) => self.billing_checkout(arguments).await,
            BillingCommand::Portal => {
                self.hosted_billing_session(
                    Endpoint::CreateBillingPortal,
                    json!({}),
                    "Billing portal URL",
                )
                .await
            }
            BillingCommand::Storage { command } => match command {
                BillingStorageCommand::Checkout(arguments) => {
                    self.storage_billing(
                        Endpoint::CreateStorageCheckout,
                        arguments,
                        "Storage checkout URL",
                    )
                    .await
                }
                BillingStorageCommand::Update(arguments) => {
                    self.storage_billing(
                        Endpoint::CreateStorageUpdate,
                        arguments,
                        "Storage update URL",
                    )
                    .await
                }
            },
            BillingCommand::Update(arguments) => self.subscription_update(arguments).await,
        }
    }

    async fn storage_billing(
        &self,
        endpoint: Endpoint,
        arguments: &StorageBillingArgs,
        label: &str,
    ) -> Result<crate::CommandResult, BlobyardError> {
        self.hosted_billing_session(
            endpoint,
            json!({ "storageBlockCount": arguments.storage_blob_count }),
            label,
        )
        .await
    }

    async fn subscription_update(
        &self,
        arguments: &BillingCheckoutArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let body = billing_plan_body(arguments)?;
        self.hosted_billing_session(
            Endpoint::CreateBillingSubscriptionUpdate,
            body,
            "Subscription update URL",
        )
        .await
    }

    async fn billing_checkout(
        &self,
        arguments: &BillingCheckoutArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let body = billing_plan_body(arguments)?;
        self.hosted_billing_session(
            Endpoint::CreateBillingCheckout,
            body,
            "Billing checkout URL",
        )
        .await
    }

    async fn execute_account(
        &self,
        command: &AccountCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            AccountCommand::Export {
                command: AccountExportCommand::Request,
            } => self.request_account_export().await,
            AccountCommand::Export {
                command: AccountExportCommand::Show,
            } => {
                self.json_read(ApiRequest::new(Endpoint::GetAccountExport))
                    .await
            }
            AccountCommand::Export {
                command: AccountExportCommand::Download(arguments),
            } => self.download_account_export(arguments).await,
            AccountCommand::Delete {
                command: AccountDeleteCommand::Show,
            } => {
                self.json_read(ApiRequest::new(Endpoint::GetAccountDeletion))
                    .await
            }
            AccountCommand::Delete {
                command: AccountDeleteCommand::Prepare,
            } => self.prepare_account_deletion().await,
            AccountCommand::Delete {
                command: AccountDeleteCommand::Complete(arguments),
            } => self.complete_account_deletion(arguments).await,
            AccountCommand::Delete {
                command: AccountDeleteCommand::Retry(arguments),
            } => self.retry_account_deletion(arguments).await,
        }
    }

    async fn download_account_export(
        &self,
        arguments: &AccountExportDownloadArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<Value>(self.mutation(Endpoint::DownloadAccountExport).with_json(
                json!({
                    "exportId": arguments.export_id,
                    "partNumber": arguments.part_number,
                }),
            ))
            .await?;
        let url = success
            .data()
            .get("downloadUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        command_result(success.data(), url, success.request_id())
    }

    async fn request_account_export(&self) -> Result<crate::CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<Value>(
                self.mutation(Endpoint::RequestAccountExport)
                    .with_json(json!({})),
            )
            .await?;
        let human = match success.data().get("status").and_then(Value::as_str) {
            Some("queued") => "Account export queued.",
            Some("running") => "Account export already running.",
            _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
        };
        command_result(success.data(), human, success.request_id())
    }

    async fn prepare_account_deletion(&self) -> Result<crate::CommandResult, BlobyardError> {
        let request = self
            .mutation(Endpoint::PrepareAccountDeletion)
            .with_json(json!({}));
        let success = self.execute_authed::<Value>(request).await?;
        let token = success
            .data()
            .get("confirmationToken")
            .and_then(Value::as_str)
            .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        let recovery_token = success
            .data()
            .get("recoveryToken")
            .and_then(Value::as_str)
            .ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        let human = format!(
            "Deletion confirmation: {token}\nDeletion recovery capability: {recovery_token}\nThe confirmation expires shortly. Keep the recovery capability until cleanup completes."
        );
        command_result(success.data(), human, success.request_id())
    }

    async fn complete_account_deletion(
        &self,
        arguments: &CompleteAccountDeletionArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let valid_token = arguments.confirmation_token.len() == 43
            && arguments.confirmation_token.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            });
        if !arguments.force || !valid_token {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "A valid confirmation token and --force are required to queue account deletion.",
            ));
        }
        self.dashboard_mutation(
            Endpoint::CompleteAccountDeletion,
            json!({ "confirmationToken": arguments.confirmation_token }),
            "Account deletion queued",
        )
        .await
    }

    async fn retry_account_deletion(
        &self,
        arguments: &RetryAccountDeletionArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        if !arguments.force {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "--force is required to retry account deletion.",
            ));
        }
        self.dashboard_mutation(
            Endpoint::RetryAccountDeletion,
            json!({ "confirmation": "DELETE MY ACCOUNT" }),
            "Account deletion running",
        )
        .await
    }

    pub(super) async fn json_read(
        &self,
        request: ApiRequest,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self.execute_authed::<Value>(request).await?;
        let human = format!("{:#}", success.data());
        command_result(success.data(), human, success.request_id())
    }

    async fn hosted_billing_session(
        &self,
        endpoint: Endpoint,
        body: Value,
        label: &str,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<BillingSessionResult>(self.mutation(endpoint).with_json(body))
            .await?;
        let human = format!("{label}: {}", success.data().url);
        command_result(success.data(), human, success.request_id())
    }

    async fn dashboard_mutation(
        &self,
        endpoint: Endpoint,
        body: Value,
        human: &str,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<Value>(self.mutation(endpoint).with_json(body))
            .await?;
        command_result(success.data(), format!("{human}."), success.request_id())
    }
}

fn billing_plan_body(arguments: &BillingCheckoutArgs) -> Result<Value, BlobyardError> {
    let seats = match (arguments.plan.as_str(), arguments.seats) {
        ("solo", None) => None,
        ("team", Some(value)) if (1..=100).contains(&value) => Some(value),
        _ => {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Solo does not accept seats. Team requires --seats between 1 and 100.",
            ));
        }
    };
    let mut body = json!({ "plan": arguments.plan.as_str() });
    if let Some(seats) = seats {
        body["seats"] = json!(seats);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixture failures must be explicit")]

    use super::*;
    use crate::runner::login::tests::support::Fixture;

    #[tokio::test]
    async fn dashboard_dispatch_fails_closed_for_unrelated_commands() {
        let fixture = Fixture::new(&["blobyard", "whoami"], vec![]);
        let error = fixture
            .runner
            .execute_dashboard_command(&Command::Whoami)
            .await
            .expect_err("unrelated dashboard command");
        assert_eq!(error.code(), ErrorCode::InternalError);
    }
}
