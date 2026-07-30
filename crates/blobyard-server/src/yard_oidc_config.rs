use crate::ServerError;
use blobyard_core::SecretString;

/// Validated optional operator configuration for the generic Yard OIDC relying party.
pub struct YardOidcConfiguration {
    issuer: openidconnect::IssuerUrl,
    client_id: String,
    client_secret: SecretString,
}

impl YardOidcConfiguration {
    /// Validates one complete OIDC configuration.
    ///
    /// # Errors
    ///
    /// Returns a stable configuration error for an invalid issuer, empty client ID, or partial
    /// configuration.
    pub fn from_optional(
        issuer: Option<String>,
        client_id: Option<String>,
        client_secret: Option<SecretString>,
    ) -> Result<Option<Self>, ServerError> {
        match (issuer, client_id, client_secret) {
            (None, None, None) => Ok(None),
            (Some(issuer), Some(client_id), Some(client_secret)) => {
                let parsed =
                    url::Url::parse(&issuer).map_err(|_error| ServerError::OidcConfiguration)?;
                blobyard_contract::normalize_oidc_issuer(parsed.as_str())
                    .filter(|normalized| normalized == &issuer)
                    .ok_or(ServerError::OidcConfiguration)?;
                let issuer = openidconnect::IssuerUrl::from_url(parsed);
                if client_id.is_empty()
                    || client_id.len() > 1_024
                    || client_id.chars().any(char::is_control)
                {
                    return Err(ServerError::OidcConfiguration);
                }
                Ok(Some(Self {
                    issuer,
                    client_id,
                    client_secret,
                }))
            }
            _ => Err(ServerError::OidcConfiguration),
        }
    }

    pub(crate) fn issuer(&self) -> &str {
        self.issuer.as_str()
    }

    pub(crate) fn issuer_url(&self) -> openidconnect::IssuerUrl {
        self.issuer.clone()
    }

    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) const fn client_secret(&self) -> &SecretString {
        &self.client_secret
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
mod tests {
    use super::YardOidcConfiguration;
    use crate::ServerError;
    use blobyard_core::SecretString;

    fn secret() -> Option<SecretString> {
        SecretString::new("fixture-secret").ok()
    }

    #[test]
    fn optional_configuration_is_all_or_nothing_and_secret_safe() {
        assert!(
            YardOidcConfiguration::from_optional(None, None, None)
                .expect("disabled")
                .is_none()
        );
        assert!(
            YardOidcConfiguration::from_optional(
                Some("https://identity.example.test/".to_owned()),
                Some("client".to_owned()),
                secret(),
            )
            .expect("enabled")
            .is_some()
        );
        let configuration = YardOidcConfiguration::from_optional(
            Some("https://identity.example.test/".to_owned()),
            Some("client".to_owned()),
            secret(),
        )
        .expect("configuration")
        .expect("enabled");
        assert_eq!(configuration.issuer(), "https://identity.example.test/");
        assert_eq!(
            configuration.issuer_url().as_str(),
            "https://identity.example.test/"
        );
        assert_eq!(configuration.client_id(), "client");
        assert_eq!(
            configuration.client_secret().expose_secret(),
            "fixture-secret"
        );
        for result in [
            YardOidcConfiguration::from_optional(
                Some("https://identity.example.test/".to_owned()),
                None,
                secret(),
            ),
            YardOidcConfiguration::from_optional(
                Some("http://identity.example.test/".to_owned()),
                Some("client".to_owned()),
                secret(),
            ),
            YardOidcConfiguration::from_optional(
                Some("not an issuer".to_owned()),
                Some("client".to_owned()),
                secret(),
            ),
            YardOidcConfiguration::from_optional(
                Some("https://identity.example.test/".to_owned()),
                Some(String::new()),
                secret(),
            ),
        ] {
            assert_eq!(result.err(), Some(ServerError::OidcConfiguration));
        }
    }
}
