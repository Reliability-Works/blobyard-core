//! Web Yard and environment command definitions.

use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Arguments for `blobyard deploy`.
#[derive(Clone, Debug, Args)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the public CLI contract exposes four independent deploy switches"
)]
pub struct DeployArgs {
    /// Static directory containing `index.html`.
    #[arg(value_name = "DIRECTORY", conflicts_with = "all")]
    pub directory: Option<PathBuf>,
    /// Named Web Yard within the selected project.
    #[arg(long, value_name = "NAME", conflicts_with = "all")]
    pub yard: Option<String>,
    /// Deploy every Web Yard configured in `.blobyard.toml`.
    #[arg(long, conflicts_with_all = ["directory", "yard"])]
    pub all: bool,
    /// Use the root entry file for unmatched extensionless paths.
    #[arg(long)]
    pub spa: bool,
    /// Resolve extensionless paths to matching HTML files.
    #[arg(long)]
    pub clean_urls: bool,
    /// Acknowledge that deployed files become public.
    #[arg(long)]
    pub public: bool,
}

/// Web Yard environment operations.
#[derive(Clone, Debug, Subcommand)]
pub enum EnvCommand {
    /// List active environments for a Web Yard.
    List(EnvListArgs),
}

/// Arguments for `blobyard env list`.
#[derive(Clone, Debug, Args)]
pub struct EnvListArgs {
    /// Project-unique Web Yard name, selected automatically when only one exists.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Web Yard management operations.
#[derive(Clone, Debug, Subcommand)]
pub enum YardCommand {
    /// List Web Yards in the selected project.
    List,
    /// Show one Web Yard, selecting it automatically when only one exists.
    Show(ShowYardArgs),
    /// List immutable deploy history for a Web Yard.
    History(YardNameArgs),
    /// Repoint a Web Yard to an earlier deploy.
    Rollback(RollbackYardArgs),
    /// Delete a Web Yard and all of its deploys.
    Delete(DeleteYardArgs),
}

/// Arguments for `blobyard yard show`.
#[derive(Clone, Debug, Args)]
pub struct ShowYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

/// Arguments selecting one named Web Yard.
#[derive(Clone, Debug, Args)]
pub struct YardNameArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
}

/// Arguments for `blobyard yard rollback`.
#[derive(Clone, Debug, Args)]
pub struct RollbackYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Specific immutable deploy identifier. The previous deploy is used when omitted.
    #[arg(value_name = "DEPLOY_ID")]
    pub deploy_id: Option<String>,
}

/// Arguments for `blobyard yard delete`.
#[derive(Clone, Debug, Args)]
pub struct DeleteYardArgs {
    /// Project-unique Web Yard name.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Confirm deletion without an interactive prompt.
    #[arg(long)]
    pub force: bool,
}
