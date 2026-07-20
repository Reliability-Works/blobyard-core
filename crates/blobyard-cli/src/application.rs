use crate::commands::Command;
use crate::{
    Cli, CommandResult, ConfigLoader, ConfigPaths, Diagnostics, Environment, OutputOptions,
    OutputRenderer, ProcessEnvironment, RenderedOutput, ResolvedConfig, Runner, TokenStore,
    generate_completion, select_token_store,
};
use blobyard_api_client::{ApiClient, ReqwestTransport, Transport};
use blobyard_core::{BlobyardError, ErrorCode};
use std::io::Write;
use std::sync::Arc;

#[path = "profile_add.rs"]
mod profile_add;

/// Explicit application seams for deterministic command behavior tests.
pub struct ApplicationDependencies {
    /// Filesystem discovery locations.
    pub paths: ConfigPaths,
    /// Read-only environment.
    pub environment: Arc<dyn Environment>,
    /// Optional token-store override; production selects the platform store.
    pub token_store: Option<Arc<dyn TokenStore>>,
}

impl std::fmt::Debug for ApplicationDependencies {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationDependencies")
            .field("paths", &self.paths)
            .field("has_token_store_override", &self.token_store.is_some())
            .finish_non_exhaustive()
    }
}

/// Runs a parsed command with production environment and filesystem discovery.
#[must_use]
pub async fn run_cli(cli: Cli) -> RenderedOutput {
    run_discovered(cli, ConfigPaths::system()).await
}

async fn run_discovered(cli: Cli, paths: Result<ConfigPaths, BlobyardError>) -> RenderedOutput {
    let options = OutputOptions::from_flags(&cli.global);
    let paths = match paths {
        Ok(paths) => paths,
        Err(error) => return OutputRenderer::new(options, Diagnostics::default()).failure(&error),
    };
    run_with(
        cli,
        ApplicationDependencies {
            paths,
            environment: Arc::new(ProcessEnvironment),
            token_store: None,
        },
    )
    .await
}

/// Runs a parsed command with explicit seams.
#[must_use]
pub async fn run_with(cli: Cli, dependencies: ApplicationDependencies) -> RenderedOutput {
    let options = OutputOptions::from_flags(&cli.global);
    if let Command::Profiles {
        command: crate::ProfilesCommand::Add(arguments),
    } = &cli.command
    {
        return profile_add::run_from_standard_input(
            &cli.global,
            arguments,
            dependencies.paths,
            dependencies.token_store,
            options,
        )
        .await;
    }
    if matches!(&cli.command, Command::Mcp { .. })
        && (cli.global.json || cli.global.quiet || cli.global.verbose)
    {
        return OutputRenderer::new(options, Diagnostics::default()).failure(&BlobyardError::new(
            ErrorCode::InvalidRequest,
            "MCP standard input and output cannot be combined with CLI output flags.",
        ));
    }
    if let Command::Completion(arguments) = &cli.command {
        let result = CommandResult::local(
            serde_json::json!({ "shell": arguments.shell.to_string() }),
            generate_completion(arguments.shell),
        );
        return OutputRenderer::new(options, Diagnostics::default()).success(result);
    }
    let config = match ConfigLoader::new(dependencies.paths, dependencies.environment.as_ref())
        .load(&cli.global)
    {
        Ok(config) => config,
        Err(error) => return OutputRenderer::new(options, Diagnostics::default()).failure(&error),
    };
    run_configured(cli, config, dependencies.token_store, options).await
}

async fn run_configured(
    cli: Cli,
    config: ResolvedConfig,
    token_store: Option<Arc<dyn TokenStore>>,
    options: OutputOptions,
) -> RenderedOutput {
    let (store, warning) = token_store.map_or_else(
        || {
            let selected = select_token_store(
                config.profile(),
                config.paths().credentials_file(config.profile()),
            );
            (selected.store(), selected.warning())
        },
        |store| (store, None),
    );
    let transport = ReqwestTransport::new(config.api().clone())
        .map(|transport| Arc::new(transport) as Arc<dyn Transport>);
    run_prepared(cli, config, store, warning, options, transport).await
}

async fn run_prepared(
    cli: Cli,
    config: ResolvedConfig,
    store: Arc<dyn TokenStore>,
    warning: Option<&'static str>,
    options: OutputOptions,
    transport: Result<Arc<dyn Transport>, BlobyardError>,
) -> RenderedOutput {
    let mut renderer = OutputRenderer::new(options, diagnostics(&config));
    if let Some(warning) = warning {
        renderer = renderer.with_warning(warning);
    }
    let transport = match transport {
        Ok(transport) => transport,
        Err(error) => return renderer.failure(&error),
    };
    let deployment = if config.profile().as_str() == "cloud" {
        blobyard_api_client::ApiDeployment::Cloud
    } else {
        blobyard_api_client::ApiDeployment::SelfHosted
    };
    let runner = Runner::new(
        ApiClient::for_deployment(transport, deployment),
        config,
        store,
    )
    .with_output_mode(options.mode())
    .with_retry_key(cli.global.retry_key.clone());
    let result = match &cli.command {
        Command::Mcp { command } => runner.serve_mcp(command.serve_arguments()).await,
        command => runner.execute(command).await,
    };
    match result {
        Ok(result) => renderer.success(result),
        Err(error) => renderer.failure(&error),
    }
}

/// Deterministic wrappers used to execute private application branches in tests.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub mod test_seams {
    use super::{run_discovered as discovered, run_prepared as prepared};
    use crate::{Cli, OutputOptions, RenderedOutput, ResolvedConfig, TokenStore};
    use blobyard_api_client::Transport;
    use blobyard_core::BlobyardError;
    use std::sync::Arc;

    /// Runs profile bootstrap with an injected secret and transport.
    pub async fn run_profile_add(
        cli: &Cli,
        paths: crate::ConfigPaths,
        store: Arc<dyn TokenStore>,
        token: blobyard_core::SecretString,
        transport: Arc<dyn Transport>,
    ) -> RenderedOutput {
        let crate::Command::Profiles {
            command: crate::ProfilesCommand::Add(arguments),
        } = &cli.command
        else {
            return crate::OutputRenderer::new(
                crate::OutputOptions::from_flags(&cli.global),
                crate::Diagnostics::default(),
            )
            .failure(&blobyard_core::BlobyardError::from_code(
                blobyard_core::ErrorCode::InvalidRequest,
            ));
        };
        super::profile_add::run(
            &cli.global,
            arguments,
            paths,
            Some(store),
            crate::OutputOptions::from_flags(&cli.global),
            token,
            Some(transport),
        )
        .await
    }

    /// Runs application discovery from an injected result.
    pub async fn run_discovered(
        cli: Cli,
        paths: Result<crate::ConfigPaths, BlobyardError>,
    ) -> RenderedOutput {
        discovered(cli, paths).await
    }

    /// Runs the prepared application path with explicit transport selection.
    pub async fn run_prepared(
        cli: Cli,
        config: ResolvedConfig,
        store: Arc<dyn TokenStore>,
        warning: Option<&'static str>,
        options: OutputOptions,
        transport: Result<Arc<dyn Transport>, BlobyardError>,
    ) -> RenderedOutput {
        prepared(cli, config, store, warning, options, transport).await
    }
}

#[cfg(test)]
#[path = "application_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "profile_add_tests.rs"]
mod profile_add_tests;

fn diagnostics(config: &ResolvedConfig) -> Diagnostics {
    Diagnostics::default()
        .with_api(config.api().api_base_url(), config.api_source())
        .with_scope(config.workspace_source(), config.project_source())
        .with_token_source(config.token_source())
}

/// Writes pre-rendered output without panicking on broken pipes.
#[must_use]
pub fn write_output(output: &RenderedOutput, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let stdout_result = stdout.write_all(output.stdout.as_bytes());
    let stderr_result = stderr.write_all(output.stderr.as_bytes());
    if stdout_result.is_ok() && stderr_result.is_ok() {
        output.exit_code
    } else {
        ErrorCode::InternalError.exit_code()
    }
}
