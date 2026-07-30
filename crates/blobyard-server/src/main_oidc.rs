#![allow(
    clippy::redundant_pub_crate,
    reason = "the private CLI module exposes its option type to the binary root"
)]

use blobyard_server::{ServerError, YardOidcConfiguration};
use clap::Args;

const CLIENT_SECRET_ENV: &str = "BLOBYARD_OIDC_CLIENT_SECRET";

/// Non-secret OIDC command-line inputs. The client secret is environment-only.
#[derive(Args, Debug)]
pub(super) struct OidcOptions {
    /// Exact generic OIDC issuer URL.
    #[arg(long)]
    oidc_issuer: Option<String>,
    /// Generic OIDC client identifier.
    #[arg(long)]
    oidc_client_id: Option<String>,
}

impl OidcOptions {
    pub(super) fn configuration(self) -> Result<Option<YardOidcConfiguration>, ServerError> {
        self.configuration_from(std::env::var_os(CLIENT_SECRET_ENV))
    }

    fn configuration_from(
        self,
        value: Option<std::ffi::OsString>,
    ) -> Result<Option<YardOidcConfiguration>, ServerError> {
        let client_secret = value
            .map(|value| {
                value
                    .into_string()
                    .map_err(|_value| ServerError::OidcConfiguration)
                    .and_then(|value| {
                        blobyard_core::SecretString::new(value)
                            .map_err(|_error| ServerError::OidcConfiguration)
                    })
            })
            .transpose()?;
        YardOidcConfiguration::from_optional(self.oidc_issuer, self.oidc_client_id, client_secret)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Arguments, oidc_cli::OidcOptions};
    use blobyard_server::ServerError;
    use clap::Parser;
    use std::ffi::OsString;

    #[test]
    fn command_line_exposes_only_non_secret_oidc_inputs() {
        let _environment_result = OidcOptions {
            oidc_issuer: None,
            oidc_client_id: None,
        }
        .configuration();
        assert!(
            Arguments::try_parse_from([
                "blobyard-server",
                "serve",
                "--oidc-issuer",
                "https://identity.example.test/",
                "--oidc-client-id",
                "blobyard-core",
            ])
            .is_ok()
        );
        assert!(
            Arguments::try_parse_from([
                "blobyard-server",
                "serve",
                "--oidc-client-secret",
                "must-not-be-an-argument",
            ])
            .is_err()
        );
        assert_eq!(
            OidcOptions {
                oidc_issuer: Some("https://identity.example.test/".to_owned()),
                oidc_client_id: Some("blobyard-core".to_owned()),
            }
            .configuration_from(Some(OsString::from("fixture-value")))
            .map(|configuration| configuration.is_some()),
            Ok(true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_environment_secret_fails_closed() {
        use std::os::unix::ffi::OsStringExt;

        assert_eq!(
            OidcOptions {
                oidc_issuer: Some("https://identity.example.test/".to_owned()),
                oidc_client_id: Some("blobyard-core".to_owned()),
            }
            .configuration_from(Some(OsString::from_vec(vec![0xff])))
            .err(),
            Some(ServerError::OidcConfiguration)
        );
    }
}
