use super::{
    YardOidcAuthorization, YardOidcExchangeFuture, YardOidcProvider, YardOidcProviderError,
    YardOidcVerifiedIdentity,
    http::{OidcHttpClient, secure_endpoint},
};
use crate::{ServerError, YardOidcConfiguration};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_core::SecretString;
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    Scope, SubjectIdentifier, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
};

#[path = "yard_oidc_provider_remote_access_token.rs"]
mod access_token;

type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[allow(
    clippy::redundant_pub_crate,
    reason = "re-exported to the sibling application module"
)]
pub(crate) struct RemoteYardOidcProvider {
    client: DiscoveredClient,
    http: OidcHttpClient,
    issuer: String,
    client_id: String,
}

impl RemoteYardOidcProvider {
    pub(crate) async fn discover(
        configuration: &YardOidcConfiguration,
        public_origin: &str,
    ) -> Result<Self, ServerError> {
        Self::discover_with_http(configuration, public_origin, OidcHttpClient::new()).await
    }

    async fn discover_with_http(
        configuration: &YardOidcConfiguration,
        public_origin: &str,
        http: Result<OidcHttpClient, ServerError>,
    ) -> Result<Self, ServerError> {
        let http = http?;
        let redirect = callback_uri(public_origin)?;
        let issuer = configuration.issuer_url();
        let metadata = CoreProviderMetadata::discover_async(issuer, &http)
            .await
            .map_err(|_error| ServerError::OidcDiscovery)?;
        validate_endpoints(&metadata)?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(configuration.client_id().to_owned()),
            Some(ClientSecret::new(
                configuration.client_secret().expose_secret().to_owned(),
            )),
        )
        .set_redirect_uri(redirect);
        Ok(Self {
            client,
            http,
            issuer: configuration.issuer().to_owned(),
            client_id: configuration.client_id().to_owned(),
        })
    }
}

impl YardOidcProvider for RemoteYardOidcProvider {
    fn authorization_url(
        &self,
        authorization: &YardOidcAuthorization,
    ) -> Result<String, YardOidcProviderError> {
        let verifier =
            PkceCodeVerifier::new(authorization.pkce_verifier.expose_secret().to_owned());
        let challenge = PkceCodeChallenge::from_code_verifier_sha256(&verifier);
        let state = authorization.state.expose_secret().to_owned();
        let nonce = authorization.nonce.expose_secret().to_owned();
        let (url, _state, _nonce) = self
            .client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                move || CsrfToken::new(state),
                move || Nonce::new(nonce),
            )
            .add_scope(Scope::new("email".to_owned()))
            .add_scope(Scope::new("profile".to_owned()))
            .set_pkce_challenge(challenge)
            .url();
        Ok(url.into())
    }

    fn exchange(
        &self,
        code: SecretString,
        nonce: SecretString,
        pkce_verifier: SecretString,
        now_ms: u64,
    ) -> YardOidcExchangeFuture<'_> {
        Box::pin(async move {
            let request = self
                .client
                .exchange_code(AuthorizationCode::new(code.expose_secret().to_owned()))
                .map_err(|_error| YardOidcProviderError::InvalidResponse)?
                .set_pkce_verifier(PkceCodeVerifier::new(
                    pkce_verifier.expose_secret().to_owned(),
                ));
            let response = request
                .request_async(&self.http)
                .await
                .map_err(|_error| YardOidcProviderError::Unavailable)?;
            verified_identity(
                &self.client,
                &self.http,
                &self.issuer,
                &self.client_id,
                &response,
                &nonce,
                now_ms,
            )
            .await
        })
    }
}

async fn verified_identity(
    client: &DiscoveredClient,
    http: &OidcHttpClient,
    issuer: &str,
    client_id: &str,
    response: &openidconnect::core::CoreTokenResponse,
    nonce: &SecretString,
    now_ms: u64,
) -> Result<YardOidcVerifiedIdentity, YardOidcProviderError> {
    let token = response
        .id_token()
        .ok_or(YardOidcProviderError::InvalidResponse)?;
    let verifier = client.id_token_verifier();
    let claims = token
        .claims(&verifier, &Nonce::new(nonce.expose_secret().to_owned()))
        .map_err(|_error| YardOidcProviderError::InvalidResponse)?;
    validate_authorized_party(claims, client_id)?;
    access_token::validate(token, claims, response)?;
    validate_not_before(token, now_ms)?;
    let subject = claims.subject().as_str();
    if !blobyard_contract::is_valid_oidc_provider_subject(subject) {
        return Err(YardOidcProviderError::InvalidResponse);
    }
    let mut email = verified_email(claims.email(), claims.email_verified());
    if email.is_none() {
        let user_info: openidconnect::core::CoreUserInfoClaims = client
            .user_info(
                response.access_token().to_owned(),
                Some(SubjectIdentifier::new(subject.to_owned())),
            )
            .map_err(|_error| YardOidcProviderError::InvalidResponse)?
            .request_async(http)
            .await
            .map_err(|_error| YardOidcProviderError::InvalidResponse)?;
        email = verified_email(user_info.email(), user_info.email_verified());
    }
    Ok(YardOidcVerifiedIdentity {
        issuer: issuer.to_owned(),
        provider_subject: subject.to_owned(),
        normalized_email: email,
    })
}

fn verified_email(
    email: Option<&openidconnect::EndUserEmail>,
    verified: Option<bool>,
) -> Option<String> {
    (verified == Some(true))
        .then(|| email.and_then(|value| blobyard_contract::normalize_oidc_email(value.as_str())))
        .flatten()
}

fn validate_authorized_party(
    claims: &openidconnect::core::CoreIdTokenClaims,
    client_id: &str,
) -> Result<(), YardOidcProviderError> {
    let authorized = claims.authorized_party().map(|value| value.as_str());
    if authorized.is_some_and(|value| value != client_id)
        || (claims.audiences().len() > 1 && authorized != Some(client_id))
    {
        Err(YardOidcProviderError::InvalidResponse)
    } else {
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct TimeClaims {
    nbf: Option<i64>,
}

fn validate_not_before(
    token: &openidconnect::core::CoreIdToken,
    now_ms: u64,
) -> Result<(), YardOidcProviderError> {
    validate_not_before_payload(&token.to_string(), now_ms)
}

fn validate_not_before_payload(raw: &str, now_ms: u64) -> Result<(), YardOidcProviderError> {
    let payload = raw
        .split('.')
        .nth(1)
        .and_then(|encoded| URL_SAFE_NO_PAD.decode(encoded).ok())
        .ok_or(YardOidcProviderError::InvalidResponse)?;
    let claims: TimeClaims = serde_json::from_slice(&payload)
        .map_err(|_error| YardOidcProviderError::InvalidResponse)?;
    let now = (now_ms / 1_000).cast_signed();
    if claims.nbf.is_none_or(|not_before| now >= not_before) {
        Ok(())
    } else {
        Err(YardOidcProviderError::InvalidResponse)
    }
}

fn callback_uri(public_origin: &str) -> Result<RedirectUrl, ServerError> {
    let mut callback =
        url::Url::parse(public_origin).map_err(|_error| ServerError::PublicOrigin)?;
    callback.set_path("/account/yard-oidc/callback");
    if secure_endpoint(&callback) {
        Ok(RedirectUrl::from_url(callback))
    } else {
        Err(ServerError::PublicOrigin)
    }
}

fn validate_endpoints(metadata: &CoreProviderMetadata) -> Result<(), ServerError> {
    let required = [
        metadata.authorization_endpoint().url(),
        metadata.jwks_uri().url(),
        metadata
            .token_endpoint()
            .ok_or(ServerError::OidcDiscovery)?
            .url(),
    ];
    if required.into_iter().all(secure_endpoint)
        && metadata
            .userinfo_endpoint()
            .is_none_or(|endpoint| secure_endpoint(endpoint.url()))
    {
        Ok(())
    } else {
        Err(ServerError::OidcDiscovery)
    }
}

#[cfg(test)]
#[path = "yard_oidc_provider_remote_edge_tests.rs"]
mod edge_tests;
#[cfg(test)]
#[path = "yard_oidc_provider_remote_integration_test_support.rs"]
mod integration_test_support;
#[cfg(test)]
#[path = "yard_oidc_provider_remote_integration_tests.rs"]
mod integration_tests;
#[cfg(test)]
#[path = "yard_oidc_provider_remote_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "yard_oidc_provider_remote_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "yard_oidc_provider_remote_transport_tests.rs"]
mod transport_tests;
