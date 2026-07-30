use blobyard_core::SecretString;
use std::{future::Future, pin::Pin, sync::Arc};

#[path = "yard_oidc_http.rs"]
mod http;
#[path = "yard_oidc_provider_remote.rs"]
mod remote;
pub(crate) use remote::RemoteYardOidcProvider;

/// Values required to construct one provider authorization redirect.
pub(crate) struct YardOidcAuthorization {
    pub(crate) state: SecretString,
    pub(crate) nonce: SecretString,
    pub(crate) pkce_verifier: SecretString,
}

/// Verified provider identity returned without retaining provider credentials.
pub(crate) struct YardOidcVerifiedIdentity {
    pub(crate) issuer: String,
    pub(crate) provider_subject: String,
    pub(crate) normalized_email: Option<String>,
}

/// Redaction-safe provider failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum YardOidcProviderError {
    Unavailable,
    InvalidResponse,
}

pub(crate) type YardOidcExchangeFuture<'a> = Pin<
    Box<dyn Future<Output = Result<YardOidcVerifiedIdentity, YardOidcProviderError>> + Send + 'a>,
>;

pub(crate) async fn configured(
    configuration: Option<&crate::YardOidcConfiguration>,
    public_origin: &str,
) -> Result<Option<Arc<dyn YardOidcProvider>>, crate::ServerError> {
    match configuration {
        Some(configuration) => Ok(Some(Arc::new(
            RemoteYardOidcProvider::discover(configuration, public_origin).await?,
        ))),
        None => Ok(None),
    }
}

/// Generic provider boundary used by the browser flow and deterministic tests.
pub(crate) trait YardOidcProvider: Send + Sync {
    fn authorization_url(
        &self,
        authorization: &YardOidcAuthorization,
    ) -> Result<String, YardOidcProviderError>;

    fn exchange(
        &self,
        code: SecretString,
        nonce: SecretString,
        pkce_verifier: SecretString,
        now_ms: u64,
    ) -> YardOidcExchangeFuture<'_>;
}
