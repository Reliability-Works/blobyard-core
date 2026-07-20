use crate::commands::Command;
use clap::{Args, Parser};
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;

/// Opaque caller-selected key used to replay an ambiguous CLI mutation.
#[derive(Clone)]
pub struct RetryKey(String);

impl RetryKey {
    pub(crate) fn expose_for_request(&self) -> &str {
        &self.0
    }
}

impl Debug for RetryKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl FromStr for RetryKey {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'-' | b'_')
            });
        valid
            .then(|| Self(value.to_owned()))
            .ok_or("use 1-128 letters, numbers, dots, colons, dashes, or underscores")
    }
}

/// Blobyard's command-line interface.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "blobyard",
    version,
    about = "Secure artifact storage for developers.",
    arg_required_else_help = true,
    disable_help_subcommand = true,
    propagate_version = true
)]
pub struct Cli {
    /// Output and endpoint options shared by every command.
    #[command(flatten)]
    pub global: GlobalArgs,
    /// The operation to perform.
    #[command(subcommand)]
    pub command: Command,
}

/// Options shared by Blobyard commands.
#[derive(Clone, Debug, Args)]
pub struct GlobalArgs {
    /// Emit one stable JSON document on standard output.
    #[arg(long, global = true)]
    pub json: bool,
    /// Suppress non-essential status and progress output.
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,
    /// Emit redacted diagnostics on standard error.
    #[arg(long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,
    /// Override the Blobyard API base URL.
    #[arg(long, global = true, value_name = "URL")]
    pub api_url: Option<String>,
    /// Override the trusted root origin used by public Web Yard subdomains.
    #[arg(long, global = true, value_name = "ORIGIN")]
    pub web_yard_origin: Option<String>,
    /// Select an isolated connection and credential profile.
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,
    /// Select a workspace slug.
    #[arg(long, global = true, value_name = "SLUG")]
    pub workspace: Option<String>,
    /// Select a project slug.
    #[arg(long, global = true, value_name = "SLUG")]
    pub project: Option<String>,
    /// Reuse one opaque key for an operation that supports durable replay.
    #[arg(long, global = true, value_name = "KEY")]
    pub retry_key: Option<RetryKey>,
}
