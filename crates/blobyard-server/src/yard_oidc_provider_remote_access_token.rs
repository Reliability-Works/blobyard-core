use crate::yard_oidc_provider::YardOidcProviderError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use openidconnect::{
    AccessToken, AccessTokenHash, OAuth2TokenResponse,
    core::{CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm, CoreTokenResponse},
};
use sha2::{Digest, Sha256, Sha384, Sha512};

pub(super) fn validate(
    token: &CoreIdToken,
    claims: &CoreIdTokenClaims,
    response: &CoreTokenResponse,
) -> Result<(), YardOidcProviderError> {
    let Some(expected) = claims.access_token_hash() else {
        return Ok(());
    };
    let actual = hash(
        token
            .signing_alg()
            .map_err(|_error| YardOidcProviderError::InvalidResponse)?,
        response.access_token(),
    )?;
    (actual == *expected)
        .then_some(())
        .ok_or(YardOidcProviderError::InvalidResponse)
}

pub(super) fn hash(
    algorithm: &CoreJwsSigningAlgorithm,
    access_token: &AccessToken,
) -> Result<AccessTokenHash, YardOidcProviderError> {
    let bytes = access_token.secret().as_bytes();
    let digest = match algorithm {
        CoreJwsSigningAlgorithm::HmacSha256
        | CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256
        | CoreJwsSigningAlgorithm::RsaSsaPssSha256
        | CoreJwsSigningAlgorithm::EcdsaP256Sha256 => Sha256::digest(bytes).to_vec(),
        CoreJwsSigningAlgorithm::HmacSha384
        | CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha384
        | CoreJwsSigningAlgorithm::RsaSsaPssSha384
        | CoreJwsSigningAlgorithm::EcdsaP384Sha384 => Sha384::digest(bytes).to_vec(),
        CoreJwsSigningAlgorithm::HmacSha512
        | CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha512
        | CoreJwsSigningAlgorithm::RsaSsaPssSha512
        | CoreJwsSigningAlgorithm::EcdsaP521Sha512 => Sha512::digest(bytes).to_vec(),
        _ => return Err(YardOidcProviderError::InvalidResponse),
    };
    Ok(AccessTokenHash::new(
        URL_SAFE_NO_PAD.encode(&digest[..digest.len() / 2]),
    ))
}
