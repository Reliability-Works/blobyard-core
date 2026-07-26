use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Local application manifest operations.
#[derive(Clone, Debug, Subcommand)]
pub enum AppCommand {
    /// Create a minimal static application manifest in the current directory.
    Init,
    /// Validate an application manifest without contacting an API.
    Validate(AppValidateArgs),
}

/// Arguments for `blobyard app validate`.
#[derive(Clone, Debug, Args)]
pub struct AppValidateArgs {
    /// Manifest path, relative to the current directory unless absolute.
    #[arg(value_name = "PATH", default_value = "blobyard.toml")]
    pub path: PathBuf,
}
