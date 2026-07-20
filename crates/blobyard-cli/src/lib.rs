//! Command grammar, configuration, API orchestration, and stable output for Blobyard.

#[cfg(test)]
extern crate self as blobyard_cli;

#[cfg(test)]
#[path = "../tests/support/request_capture.rs"]
#[doc(hidden)]
pub mod request_capture;

mod account_commands;
mod application;
mod args;
mod billing_commands;
mod commands;
mod completion;
mod config;
mod headless_commands;
mod output;
mod runner;
mod token_store;

#[cfg(test)]
mod runner_cases_tests;

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub use application::test_seams;
pub use application::{ApplicationDependencies, run_cli, run_with, write_output};
pub use args::{Cli, GlobalArgs, RetryKey};
pub use commands::{Command, CompletionShell, ProfileAddArgs, ProfilesCommand};
pub use completion::generate_completion;
pub use config::{
    ConfigLoader, ConfigPaths, ConfigSource, Environment, ProcessEnvironment, ResolvedConfig,
    YardConfig,
};
pub use output::{
    CommandResult, Diagnostics, OutputMode, OutputOptions, OutputRenderer, RenderedOutput,
};
pub use runner::Runner;
pub use token_store::{
    FILE_FALLBACK_WARNING, FileTokenStore, PlatformTokenStore, SelectedTokenStore, TokenStore,
    select_token_store,
};
