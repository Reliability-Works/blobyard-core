//! Generated `OpenAPI` adapters must resolve to real Clap command paths.

use blobyard_api_client::Endpoint;
use blobyard_cli::Cli;
use clap::{CommandFactory, Parser, error::ErrorKind};

include!("generated/openapi_operations.rs");

#[test]
fn every_openapi_cli_adapter_is_a_real_command() -> Result<(), String> {
    assert!(!OPENAPI_CLI_OPERATIONS.is_empty());
    for (operation, expected_path, expected_method, idempotency, path) in OPENAPI_CLI_OPERATIONS {
        let endpoint = Endpoint::PUBLIC
            .into_iter()
            .find(|endpoint| endpoint.operation_id() == *operation)
            .ok_or_else(|| format!("{operation} has no public API endpoint"))?;
        assert_eq!(endpoint.path(), *expected_path, "{operation} path drifted");
        assert_eq!(
            endpoint.method().as_str(),
            *expected_method,
            "{operation} method drifted"
        );
        assert_eq!(
            endpoint.supports_idempotency(),
            *idempotency,
            "{operation} idempotency metadata drifted"
        );
        let mut command = Cli::command();
        for part in *path {
            let subcommand = command.find_subcommand(part).ok_or_else(|| {
                format!("{operation} references missing CLI path {}", path.join(" "))
            })?;
            command = subcommand.clone();
        }
        let mut arguments = vec!["blobyard"];
        arguments.extend(path.iter().copied());
        let result = Cli::try_parse_from(arguments);
        if let Err(error) = result {
            assert_ne!(
                error.kind(),
                ErrorKind::InvalidSubcommand,
                "{operation} references invalid CLI path {}",
                path.join(" ")
            );
        }
    }
    Ok(())
}
