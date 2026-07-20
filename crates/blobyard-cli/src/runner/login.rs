use super::{Runner, api_error, command_result};
use crate::commands::LoginArgs;
use blobyard_api_client::{
    ApiRequest, DevicePollRequest, DevicePollResponse, DevicePollState, DeviceStartRequest,
    DeviceStartResponse, Endpoint, TokenPair,
};
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use std::ffi::OsString;
use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::process::Command;
use std::time::Duration;
use url::Url;

pub(super) fn auth_required() -> BlobyardError {
    BlobyardError::from_code(ErrorCode::AuthRequired)
}

/// Device-login interaction seam kept behind the private runner module.
pub(super) trait LoginPort: Send + Sync {
    /// Attempts to open the activation URL without making failure fatal.
    fn open_browser(&self, url: &str) -> bool;
    /// Presents the canonical verification URL and ambiguity-free user code.
    fn present(&self, verification_uri: &str, user_code: &SecretString);
    /// Waits without blocking the async runtime.
    fn wait(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Production terminal, browser, and async-wait adapter.
pub(super) struct SystemLoginPort {
    browser_program: Option<OsString>,
}

impl Default for SystemLoginPort {
    fn default() -> Self {
        Self {
            browser_program: std::env::var_os("BROWSER"),
        }
    }
}

impl LoginPort for SystemLoginPort {
    fn open_browser(&self, url: &str) -> bool {
        launch_command(browser_command(url, self.browser_program.as_ref()))
    }

    fn present(&self, verification_uri: &str, user_code: &SecretString) {
        let message = instruction_text(verification_uri, user_code.expose_secret());
        let _ = writeln!(std::io::stderr().lock(), "{message}");
    }

    fn wait(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

impl std::fmt::Debug for SystemLoginPort {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SystemLoginPort")
    }
}

impl Runner {
    pub(super) async fn login(
        &self,
        arguments: &LoginArgs,
    ) -> Result<super::CommandResult, BlobyardError> {
        let started = self.start_device(arguments).await?;
        let activation = activation_url(
            &started.data().verification_uri,
            started.data().user_code.expose_secret(),
        )?;
        self.login_port
            .present(&started.data().verification_uri, &started.data().user_code);
        if !arguments.no_open {
            let _ = self.login_port.open_browser(activation.as_str());
        }
        self.poll_device(started.data()).await
    }

    async fn start_device(
        &self,
        arguments: &LoginArgs,
    ) -> Result<blobyard_api_client::ApiSuccess<DeviceStartResponse>, BlobyardError> {
        let request = DeviceStartRequest {
            name: arguments
                .name
                .clone()
                .unwrap_or_else(|| "Blobyard CLI".to_owned()),
            platform: std::env::consts::OS.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        self.api
            .execute(ApiRequest::new(Endpoint::DeviceStart).with_json(request.into_json()))
            .await
            .map_err(api_error)
    }

    async fn poll_device(
        &self,
        started: &DeviceStartResponse,
    ) -> Result<super::CommandResult, BlobyardError> {
        let mut delay = poll_delay(started.poll_interval_seconds)?;
        loop {
            self.login_port.wait(delay).await;
            match self.poll_once(&started.device_code).await {
                Ok(success) => match poll_state(success.data())? {
                    DevicePollState::Pending => {}
                    DevicePollState::SlowDown => delay = slow_down(delay),
                    DevicePollState::Denied => return Err(login_denied()),
                    DevicePollState::Expired => return Err(login_expired()),
                    DevicePollState::Approved => {
                        return self.finish_login(success.data(), success.request_id());
                    }
                },
                Err(error) => return Err(error.into_error()),
            }
        }
    }

    async fn poll_once(
        &self,
        device_code: &SecretString,
    ) -> Result<
        blobyard_api_client::ApiSuccess<DevicePollResponse>,
        blobyard_api_client::ApiCallError,
    > {
        let body = DevicePollRequest {
            device_code: device_code.clone(),
        }
        .into_json();
        self.api
            .execute(ApiRequest::new(Endpoint::DevicePoll).with_json(body))
            .await
    }

    fn finish_login(
        &self,
        response: &DevicePollResponse,
        request_id: &str,
    ) -> Result<super::CommandResult, BlobyardError> {
        let tokens = approved_tokens(response)?;
        self.token_store.save(&tokens.refresh_token)?;
        command_result(
            &serde_json::json!({ "status": "signed_in" }),
            "Signed in to Blobyard.",
            request_id,
        )
    }

    #[cfg(test)]
    fn with_login_port(mut self, port: std::sync::Arc<dyn LoginPort>) -> Self {
        self.login_port = port;
        self
    }
}

fn approved_tokens(response: &DevicePollResponse) -> Result<&TokenPair, BlobyardError> {
    response.tokens.as_ref().ok_or_else(invalid_device_response)
}

fn poll_state(response: &DevicePollResponse) -> Result<DevicePollState, BlobyardError> {
    let approved = response.status == DevicePollState::Approved;
    if approved == response.tokens.is_some() {
        Ok(response.status.clone())
    } else {
        Err(invalid_device_response())
    }
}

fn activation_url(base: &str, user_code: &str) -> Result<Url, BlobyardError> {
    let mut url = Url::parse(base).map_err(|_| invalid_device_response())?;
    url.query_pairs_mut().append_pair("user_code", user_code);
    Ok(url)
}

fn poll_delay(seconds: u16) -> Result<Duration, BlobyardError> {
    (seconds > 0)
        .then(|| Duration::from_secs(u64::from(seconds)))
        .ok_or_else(invalid_device_response)
}

const fn slow_down(current: Duration) -> Duration {
    current.saturating_add(Duration::from_secs(5))
}

fn instruction_text(verification_uri: &str, user_code: &str) -> String {
    format!("Open {verification_uri} and enter code {user_code}.")
}

fn invalid_device_response() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::ProviderUnavailable,
        "Blobyard returned an invalid device-login response. Start login again.",
    )
}

fn login_denied() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::Forbidden,
        "Device login was denied in the browser.",
    )
}

fn login_expired() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::TokenExpired,
        "Device login expired. Run blobyard login again.",
    )
}

#[cfg(target_os = "macos")]
fn browser_command(url: &str, override_program: Option<&OsString>) -> Command {
    let mut command =
        Command::new(override_program.map_or_else(|| OsString::from("open"), Clone::clone));
    command.arg(url);
    command
}

#[cfg(target_os = "linux")]
fn browser_command(url: &str, override_program: Option<&OsString>) -> Command {
    let mut command =
        Command::new(override_program.map_or_else(|| OsString::from("xdg-open"), Clone::clone));
    command.arg(url);
    command
}

#[cfg(target_os = "windows")]
fn browser_command(url: &str, override_program: Option<&OsString>) -> Command {
    let mut command =
        Command::new(override_program.map_or_else(|| OsString::from("rundll32"), Clone::clone));
    if override_program.is_some() {
        command.arg(url);
    } else {
        command.args(["url.dll,FileProtocolHandler", url]);
    }
    command
}

fn launch_command(mut command: Command) -> bool {
    command.status().is_ok()
}

#[cfg(test)]
#[path = "login_tests/mod.rs"]
pub(in crate::runner) mod tests;
