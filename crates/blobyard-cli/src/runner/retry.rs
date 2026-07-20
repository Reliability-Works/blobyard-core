use super::Runner;
use crate::RetryKey;
use blobyard_api_client::{ApiRequest, Endpoint};
use sha2::{Digest, Sha256};

impl Runner {
    pub(super) fn mutation(&self, endpoint: Endpoint) -> ApiRequest {
        mutation_request(endpoint, self.retry_key.as_ref())
    }
}

fn mutation_request(endpoint: Endpoint, retry_key: Option<&RetryKey>) -> ApiRequest {
    let request = ApiRequest::new(endpoint);
    if !endpoint.supports_idempotency() {
        return request;
    }
    match retry_key {
        None => request.with_generated_idempotency_key(),
        Some(retry_key) => request.with_deterministic_idempotency_key(retry_digest(
            endpoint,
            retry_key.expose_for_request(),
        )),
    }
}

fn retry_digest(endpoint: Endpoint, retry_key: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"blobyard:cli-retry:v1\0");
    digest.update(endpoint.path().as_bytes());
    digest.update(b"\0");
    digest.update(retry_key.as_bytes());
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::mutation_request;
    use crate::RetryKey;
    use blobyard_api_client::Endpoint;

    #[test]
    fn missing_retry_key_generates_a_fresh_idempotency_key() {
        let first = mutation_request(Endpoint::PrepareAccountDeletion, None);
        let second = mutation_request(Endpoint::PrepareAccountDeletion, None);
        let first_key = first.idempotency_key().expect("first key");
        let second_key = second.idempotency_key().expect("second key");

        assert!(first_key.starts_with("blobyard-client-"));
        assert!(second_key.starts_with("blobyard-client-"));
        assert_ne!(first_key, second_key);
    }

    #[test]
    fn retry_key_is_deterministic_endpoint_scoped_and_opaque() {
        let raw_retry_key = "account-delete-20260715";
        let retry_key = raw_retry_key.parse::<RetryKey>().expect("retry key");
        let first = mutation_request(Endpoint::PrepareAccountDeletion, Some(&retry_key));
        let replay = mutation_request(Endpoint::PrepareAccountDeletion, Some(&retry_key));
        let other_endpoint = mutation_request(Endpoint::CompleteAccountDeletion, Some(&retry_key));
        let first_key = first.idempotency_key().expect("first key");

        assert_eq!(first_key, replay.idempotency_key().expect("replay key"));
        assert_ne!(
            first_key,
            other_endpoint.idempotency_key().expect("other key")
        );
        assert!(first_key.starts_with("blobyard-digest-"));
        assert!(!first_key.contains(raw_retry_key));
        assert!(!first_key.contains("deletion"));
    }

    #[test]
    fn unsupported_mutations_never_receive_retry_keys() {
        let retry_key = "token-create".parse::<RetryKey>().expect("retry key");
        let fresh = mutation_request(Endpoint::CreateApiToken, None);
        let caller_selected = mutation_request(Endpoint::CreateApiToken, Some(&retry_key));

        assert_eq!(fresh.idempotency_key(), None);
        assert_eq!(caller_selected.idempotency_key(), None);
    }
}
