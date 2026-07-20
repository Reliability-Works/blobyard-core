//! Local operator entry point for the single-node Blob Yard service.

use blobyard_server::{
    HostedMigrationOptions, backup_data_directory, enforce_retention_with_storage, initialize,
    migrate_from_hosted, reconcile_data_directory, restore_data_directory, rollback_preflight,
    serve_until_with_storage, show_new_token, upgrade_preflight,
};
use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

#[path = "main_storage.rs"]
/// Command-line object-storage configuration shared by operator subcommands.
pub mod storage_cli;
use storage_cli::StorageOptions;

#[derive(Debug, Parser)]
#[command(name = "blobyard-server", version)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the standalone HTTP service.
    Serve {
        /// HTTP listen address.
        #[arg(long, default_value = "127.0.0.1:8787")]
        listen: SocketAddr,
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// Public root URL used in short-lived transfer grants.
        #[arg(long)]
        public_url: Option<String>,
        /// Trusted root origin whose first-level subdomains serve public Web Yards.
        #[arg(long)]
        web_yard_origin: Option<String>,
        /// Durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Generate first-start bootstrap authority without starting HTTP.
    BootstrapToken {
        /// Confirm creation for a never-initialized data directory.
        #[arg(long)]
        generate: bool,
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
    },
    /// Enforce enabled retention policies once for cron or a timer.
    RetentionEnforce {
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// Durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Report metadata and physical object integrity without changing data.
    Reconcile {
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// Durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Create a verified portable backup in a new directory.
    Backup {
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// New backup directory. Existing paths are never replaced.
        #[arg(long)]
        output: PathBuf,
        /// Durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Restore a verified backup into an absent installation.
    Restore {
        /// Backup directory containing manifest, metadata, secret, and objects.
        #[arg(long)]
        input: PathBuf,
        /// New durable metadata and object root. Existing paths are never replaced.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// Empty durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Verify that this binary can upgrade an installation without changing it.
    UpgradePreflight {
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
    },
    /// Verify that this exact binary supports a code-only rollback.
    RollbackPreflight {
        /// Durable metadata and object root.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
    },
    /// Migrate selected Blob Yard Cloud workspaces into an absent standalone installation.
    HostedMigrate {
        /// Blob Yard Cloud API origin.
        #[arg(long, default_value = "https://api.blobyard.com")]
        source_url: String,
        /// Read one Cloud API token from standard input. Tokens are never accepted on argv.
        #[arg(long)]
        token_stdin: bool,
        /// Active workspace slug to migrate. Repeat to select multiple; omit to select all.
        #[arg(long = "workspace")]
        workspaces: Vec<String>,
        /// New standalone installation directory. Existing paths are never replaced.
        #[arg(long, default_value = ".blobyard")]
        data_dir: PathBuf,
        /// Public standalone origin used for replacement share URLs.
        #[arg(long, default_value = "http://127.0.0.1:8787")]
        public_url: String,
        /// Empty durable object-storage backend.
        #[command(flatten)]
        storage: StorageOptions,
    },
    /// Check one running standalone service readiness endpoint.
    Healthcheck {
        /// Exact readiness URL.
        #[arg(long, default_value = "http://127.0.0.1:8787/v1/health")]
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_command(Arguments::parse().command).await
}

async fn run_command(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Serve {
            listen,
            data_dir,
            public_url,
            web_yard_origin,
            storage,
        } => {
            serve(listen, data_dir, public_url, web_yard_origin, storage).await?;
        }
        Command::BootstrapToken { generate, data_dir } => bootstrap(generate, &data_dir)?,
        Command::RetentionEnforce { data_dir, storage } => {
            enforce_retention_with_storage(&data_dir, &storage.configuration()?)?;
        }
        Command::Reconcile { data_dir, storage } => {
            let report = reconcile_data_directory(&data_dir, &storage.configuration()?)?;
            write_report(&report)?;
        }
        Command::Backup {
            data_dir,
            output,
            storage,
        } => {
            let report = backup_data_directory(&data_dir, &output, &storage.configuration()?)?;
            write_report(&report)?;
        }
        Command::Restore {
            input,
            data_dir,
            storage,
        } => {
            let report = restore_data_directory(&input, &data_dir, &storage.configuration()?)?;
            write_report(&report)?;
        }
        Command::UpgradePreflight { data_dir } => write_report(&upgrade_preflight(&data_dir)?)?,
        Command::RollbackPreflight { data_dir } => write_report(&rollback_preflight(&data_dir)?)?,
        Command::HostedMigrate {
            source_url,
            token_stdin,
            workspaces,
            data_dir,
            public_url,
            storage,
        } => {
            hosted_migrate(
                source_url,
                data_dir,
                public_url,
                workspaces,
                storage,
                token_stdin,
            )
            .await?;
        }
        Command::Healthcheck { url } => healthcheck(&url).await?,
    }
    Ok(())
}

async fn healthcheck(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    reqwest::get(url).await?.error_for_status()?;
    Ok(())
}

async fn serve(
    listen: SocketAddr,
    data_dir: PathBuf,
    public_url: Option<String>,
    web_yard_origin: Option<String>,
    storage: StorageOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = storage.configuration()?;
    serve_until_with_storage(
        listen,
        &data_dir,
        public_url.as_deref(),
        web_yard_origin.as_deref(),
        &storage,
        Box::pin(std::future::pending()),
    )
    .await
}

async fn hosted_migrate(
    source_url: String,
    data_dir: PathBuf,
    public_url: String,
    workspaces: Vec<String>,
    storage: StorageOptions,
    token_stdin: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = read_source_token(token_stdin)?;
    let options = HostedMigrationOptions::new(
        source_url,
        data_dir,
        public_url,
        workspaces,
        storage.configuration()?,
    );
    write_report(&migrate_from_hosted(&options, token).await?)?;
    Ok(())
}

fn read_source_token(
    enabled: bool,
) -> Result<blobyard_core::SecretString, Box<dyn std::error::Error>> {
    read_source_token_from(&mut std::io::stdin(), enabled)
}

fn read_source_token_from(
    input: &mut dyn Read,
    enabled: bool,
) -> Result<blobyard_core::SecretString, Box<dyn std::error::Error>> {
    if !enabled {
        return Err("hosted-migrate requires --token-stdin".into());
    }
    let mut value = String::new();
    input.take(16_385).read_to_string(&mut value)?;
    if value.len() > 16_384 {
        return Err("source token input is too large".into());
    }
    let value = value.trim_end_matches(['\r', '\n']).to_owned();
    blobyard_core::SecretString::new(value).map_err(Into::into)
}

fn write_report(report: &str) -> std::io::Result<()> {
    let standard_output = std::io::stdout();
    let mut output = standard_output.lock();
    output.write_all(report.as_bytes())?;
    output.write_all(b"\n")
}

fn bootstrap(
    generate: bool,
    data_directory: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !generate {
        return Err("bootstrap-token requires --generate".into());
    }
    let mut initialized = initialize(data_directory)?;
    match initialized.take_bootstrap_token() {
        Some(token) => show_new_token(Some(token)),
        None => return Err("bootstrap authority is already initialized or consumed".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{Arguments, read_source_token_from};
    use clap::Parser;
    use std::io::{Error, Read};

    struct FailedReader;

    impl Read for FailedReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Err(Error::other("fixture read failure"))
        }
    }

    #[test]
    fn command_line_accepts_operator_commands_and_storage_flags() {
        let _reconcile = Arguments::try_parse_from([
            "blobyard-server",
            "reconcile",
            "--data-dir",
            "data",
            "--storage",
            "s3",
            "--s3-endpoint",
            "http://localhost:9000",
            "--s3-bucket",
            "bucket",
            "--s3-force-path-style",
        ])
        .expect("reconcile arguments");
        for arguments in [
            vec!["blobyard-server", "backup", "--output", "backup"],
            vec!["blobyard-server", "restore", "--input", "backup"],
            vec!["blobyard-server", "upgrade-preflight"],
            vec!["blobyard-server", "rollback-preflight"],
            vec!["blobyard-server", "hosted-migrate", "--token-stdin"],
            vec!["blobyard-server", "healthcheck"],
        ] {
            assert!(Arguments::try_parse_from(arguments).is_ok());
        }
    }

    #[test]
    fn source_token_reader_propagates_input_failures() {
        assert!(read_source_token_from(&mut FailedReader, true).is_err());
    }
}
