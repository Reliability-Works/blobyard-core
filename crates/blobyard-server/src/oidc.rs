use blobyard_contract::GithubOidcIdentity;
use futures_util::future::BoxFuture;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[path = "oidc_claims.rs"]
mod claims;
#[path = "oidc_jwt.rs"]
mod jwt;
use jwt::{Jwk, JwkSet};

const GITHUB_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
const GITHUB_OIDC_JWKS_URL: &str = "https://token.actions.githubusercontent.com/.well-known/jwks";
const CACHE_TTL_MS: u64 = 10 * 60 * 1_000;
const COOLDOWN_MS: u64 = 30 * 1_000;
const MAX_JWKS_BYTES: usize = 256 * 1_024;

/// Validates serialized GitHub claims without network or signature handling.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub fn verify_test_claims(
    value: serde_json::Value,
    audience: &str,
    now_ms: u64,
) -> Result<GithubOidcIdentity, OidcVerificationError> {
    identity_from_value(value, audience, now_ms)
}

fn identity_from_value(
    value: serde_json::Value,
    audience: &str,
    now_ms: u64,
) -> Result<GithubOidcIdentity, OidcVerificationError> {
    let claims = serde_json::from_value(value).map_err(|_error| OidcVerificationError::Invalid)?;
    claims::identity(claims, audience, now_ms)
}

/// Stable verification failure classes safe to map onto the public HTTP contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OidcVerificationError {
    /// The assertion, signature, key, or claims are invalid.
    Invalid,
    /// GitHub's key provider is temporarily unavailable.
    ProviderUnavailable,
}

/// Verifies GitHub-signed OIDC assertions without exposing their raw contents.
pub trait GithubOidcVerifier: Send + Sync {
    /// Verifies one assertion for the exact configured audience and time.
    fn verify<'a>(
        &'a self,
        token: &'a str,
        audience: &'a str,
        now_ms: u64,
    ) -> BoxFuture<'a, Result<GithubOidcIdentity, OidcVerificationError>>;
}

/// Production verifier backed by GitHub's bounded remote JWKS endpoint.
pub struct RemoteGithubOidcVerifier {
    client: reqwest::Client,
    cache: Arc<RwLock<KeyCache>>,
    jwks_url: Arc<str>,
}

#[derive(Default)]
struct KeyCache {
    expires_at_ms: u64,
    keys: Option<JwkSet>,
    retry_after_ms: u64,
}

impl RemoteGithubOidcVerifier {
    /// Creates an isolated verifier with no preloaded keys.
    #[must_use]
    pub fn new() -> Self {
        Self::with_jwks_url(GITHUB_OIDC_JWKS_URL)
    }

    /// Creates a verifier against an isolated JWKS endpoint for contract tests.
    #[cfg(any(test, feature = "test-seams"))]
    #[doc(hidden)]
    #[must_use]
    pub fn with_test_jwks_url(jwks_url: &str) -> Self {
        Self::with_jwks_url(jwks_url)
    }

    fn with_jwks_url(jwks_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(KeyCache::default())),
            jwks_url: Arc::from(jwks_url),
        }
    }

    async fn key(&self, token: &str, now_ms: u64) -> Result<Jwk, OidcVerificationError> {
        let kid = jwt::key_id(token)?;
        let cached = self.keys(now_ms, false).await?;
        if let Some(key) = cached.find(&kid) {
            return valid_key(key);
        }
        let refreshed = self.keys(now_ms, true).await?;
        refreshed
            .find(&kid)
            .ok_or(OidcVerificationError::Invalid)
            .and_then(valid_key)
    }

    async fn keys(&self, now_ms: u64, force: bool) -> Result<JwkSet, OidcVerificationError> {
        let cache = self.cache.read().await;
        if !force
            && cache.expires_at_ms > now_ms
            && let Some(keys) = cache.keys.clone()
        {
            return Ok(keys);
        }
        if cache.retry_after_ms > now_ms {
            if force && let Some(keys) = cache.keys.clone() {
                return Ok(keys);
            }
            return Err(OidcVerificationError::ProviderUnavailable);
        }
        drop(cache);
        match self.fetch_keys().await {
            Ok(keys) => {
                let mut cache = self.cache.write().await;
                cache.expires_at_ms = now_ms.saturating_add(CACHE_TTL_MS);
                cache.retry_after_ms = now_ms.saturating_add(COOLDOWN_MS);
                cache.keys = Some(keys.clone());
                drop(cache);
                Ok(keys)
            }
            Err(error) => {
                let mut cache = self.cache.write().await;
                cache.retry_after_ms = now_ms.saturating_add(COOLDOWN_MS);
                drop(cache);
                Err(error)
            }
        }
    }

    async fn fetch_keys(&self) -> Result<JwkSet, OidcVerificationError> {
        let response = self
            .client
            .get(self.jwks_url.as_ref())
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|_error| OidcVerificationError::ProviderUnavailable)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > MAX_JWKS_BYTES as u64)
        {
            return Err(OidcVerificationError::ProviderUnavailable);
        }
        let body = response
            .bytes()
            .await
            .map_err(|_error| OidcVerificationError::ProviderUnavailable)?;
        if body.len() > MAX_JWKS_BYTES {
            return Err(OidcVerificationError::ProviderUnavailable);
        }
        serde_json::from_slice(&body).map_err(|_error| OidcVerificationError::ProviderUnavailable)
    }
}

impl Default for RemoteGithubOidcVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubOidcVerifier for RemoteGithubOidcVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        audience: &'a str,
        now_ms: u64,
    ) -> BoxFuture<'a, Result<GithubOidcIdentity, OidcVerificationError>> {
        Box::pin(async move {
            let key = self.key(token, now_ms).await?;
            verify_with_key(token, &key, audience, now_ms)
        })
    }
}

fn verify_with_key(
    token: &str,
    key: &Jwk,
    audience: &str,
    now_ms: u64,
) -> Result<GithubOidcIdentity, OidcVerificationError> {
    let claims = jwt::verify(token, key)?;
    identity_from_value(claims, audience, now_ms)
}

fn valid_key(key: &Jwk) -> Result<Jwk, OidcVerificationError> {
    if key.alg.as_deref() == Some("RS256")
        && key.kty.as_deref() == Some("RSA")
        && key
            .public_key_use
            .as_deref()
            .is_none_or(|usage| usage == "sig")
        && key.e.as_deref().is_some_and(|value| !value.is_empty())
        && key.n.as_deref().is_some_and(|value| !value.is_empty())
    {
        Ok(key.clone())
    } else {
        Err(OidcVerificationError::Invalid)
    }
}

/// Deterministic verifier used by tests that do not exercise OIDC.
pub struct UnavailableGithubOidcVerifier;

impl GithubOidcVerifier for UnavailableGithubOidcVerifier {
    fn verify<'a>(
        &'a self,
        _token: &'a str,
        _audience: &'a str,
        _now_ms: u64,
    ) -> BoxFuture<'a, Result<GithubOidcIdentity, OidcVerificationError>> {
        Box::pin(async { Err(OidcVerificationError::ProviderUnavailable) })
    }
}

#[cfg(test)]
#[path = "oidc_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "oidc_remote_tests.rs"]
mod remote_tests;
