use blobyard_contract::{NewYardOidcAttempt, YARD_OIDC_ATTEMPT_LIFETIME_MS};
use blobyard_core::{GeneratedSecretKind, SecretString};
use hmac::{Hmac, Mac};
use sha2::Sha256;

const NONCE_DOMAIN: &[u8] = b"yard-oidc-nonce-v1";
const PKCE_DOMAIN: &[u8] = b"yard-oidc-pkce-v1";
type HmacSha256 = Hmac<Sha256>;

pub(crate) struct DerivedAuthorization {
    pub(crate) nonce: SecretString,
    pub(crate) pkce_verifier: SecretString,
}

pub(crate) fn generate_state() -> SecretString {
    crate::auth::generate_token(GeneratedSecretKind::YardOidcState)
}

pub(crate) fn state_shape(value: &str) -> bool {
    crate::yard_session_contracts::has_token_shape(value, blobyard_contract::YARD_OIDC_STATE_PREFIX)
}

pub(crate) fn derive(key: &[u8; 32], state: &SecretString) -> DerivedAuthorization {
    DerivedAuthorization {
        nonce: derived_secret(
            key,
            NONCE_DOMAIN,
            state.expose_secret(),
            GeneratedSecretKind::YardOidcNonce,
        ),
        pkce_verifier: derived_secret(
            key,
            PKCE_DOMAIN,
            state.expose_secret(),
            GeneratedSecretKind::YardOidcPkceVerifier,
        ),
    }
}

pub(crate) fn attempt(
    state: &SecretString,
    continuation: &SecretString,
    host_label: &str,
    return_path: &str,
    now_ms: u64,
) -> NewYardOidcAttempt {
    NewYardOidcAttempt {
        state_hash: crate::auth::hash(state.expose_secret()),
        continuation_hash: crate::auth::hash(continuation.expose_secret()),
        host_label: host_label.to_owned(),
        return_path: return_path.to_owned(),
        created_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(YARD_OIDC_ATTEMPT_LIFETIME_MS),
    }
}

fn derived_secret(
    key: &[u8; 32],
    domain: &[u8],
    state: &str,
    kind: GeneratedSecretKind,
) -> SecretString {
    let mut padded_key = [0_u8; 64];
    padded_key[..key.len()].copy_from_slice(key);
    let mut mac = HmacSha256::new((&padded_key).into());
    mac.update(domain);
    mac.update(&[0]);
    mac.update(state.as_bytes());
    SecretString::from_generated_entropy(kind, mac.finalize().into_bytes().into())
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
mod tests {
    use super::{derive, state_shape};
    use blobyard_core::SecretString;

    #[test]
    fn state_shape_and_purpose_bound_derivations_are_exact() {
        let state =
            SecretString::new(format!("byos_{}", "a".repeat(64))).expect("valid state secret");
        assert!(state_shape(state.expose_secret()));
        assert!(!state_shape(&format!("byx_{}", "a".repeat(64))));
        let first = derive(&[7; 32], &state);
        let repeated = derive(&[7; 32], &state);
        assert_eq!(first.nonce, repeated.nonce);
        assert_eq!(first.pkce_verifier, repeated.pkce_verifier);
        assert_ne!(first.nonce, first.pkce_verifier);
        assert_eq!(first.nonce.expose_secret().len(), 69);
        assert_eq!(first.pkce_verifier.expose_secret().len(), 69);
    }
}
