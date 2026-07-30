use crate::{AuditValue, RepositoryError};

/// Absolute lifetime of one OIDC authorization attempt.
pub const YARD_OIDC_ATTEMPT_LIFETIME_MS: u64 = 600_000;
/// Maximum expired or claimed attempts removed by one bounded housekeeping pass.
pub const YARD_OIDC_ATTEMPT_CLEANUP_LIMIT: usize = 100;
/// Prefix for one raw OIDC state handle returned only through the browser.
pub const YARD_OIDC_STATE_PREFIX: &str = "byos_";
/// Stable audit action emitted exactly once for a new provider binding.
pub const YARD_OIDC_IDENTITY_LINKED_ACTION: &str = "yard.oidc_identity_linked";
/// Stable audit target for a new provider binding.
pub const YARD_OIDC_IDENTITY_AUDIT_TARGET: &str = "yard_oidc_identity";

/// Parses and canonicalizes one secure OIDC issuer.
///
/// HTTPS is required except for loopback HTTP issuers used by local operators and tests.
#[must_use]
pub fn normalize_oidc_issuer(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host()?;
    let loopback = match host {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    let secure_scheme = parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback);
    (secure_scheme
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none())
    .then(|| parsed.to_string())
}

/// Normalizes one provider email and rejects values unsuitable for identity binding.
#[must_use]
pub fn normalize_oidc_email(value: &str) -> Option<String> {
    let normalized = value.trim().to_lowercase();
    let valid = (3..=254).contains(&normalized.len())
        && !normalized
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        && normalized.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        });
    valid.then_some(normalized)
}

/// Validates one bounded non-empty OIDC subject claim.
#[must_use]
pub fn is_valid_oidc_provider_subject(value: &str) -> bool {
    (1..=512).contains(&value.len()) && !value.chars().any(char::is_control)
}

/// Hashed durable state for one validated OIDC authorization start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardOidcAttempt {
    /// Lowercase SHA-256 digest of the raw state handle.
    pub state_hash: String,
    /// Lowercase SHA-256 digest of the signed Yard continuation.
    pub continuation_hash: String,
    /// Exact validated Yard host carried by the continuation.
    pub host_label: String,
    /// Normalized return path carried by the continuation.
    pub return_path: String,
    /// Attempt creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Absolute attempt expiry as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Persisted OIDC attempt returned after its single atomic claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardOidcAttemptRecord {
    /// Durable non-secret start values.
    pub attempt: NewYardOidcAttempt,
    /// Atomic claim time.
    pub claimed_at_ms: Option<u64>,
}

/// Stable provider identity bound to one existing Yard runtime subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardOidcIdentityRecord {
    /// Exact validated provider issuer.
    pub issuer: String,
    /// Exact non-empty provider subject.
    pub provider_subject: String,
    /// Workspace tenant boundary.
    pub workspace_id: String,
    /// Existing opaque Yard runtime subject.
    pub yard_subject_id: String,
    /// Verified normalized email observed at first binding.
    pub normalized_email: String,
    /// Binding creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Most recent successful OIDC authentication time.
    pub last_authenticated_at_ms: u64,
}

/// Validated external identity presented for binding or returning authentication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardOidcAuthentication {
    /// Exact discovered and ID-token issuer.
    pub issuer: String,
    /// Exact non-empty ID-token subject.
    pub provider_subject: String,
    /// Affirmatively verified normalized email, or absent when verification failed.
    pub normalized_email: Option<String>,
    /// Exact Yard host resumed from the claimed attempt.
    pub host_label: String,
    /// Authentication time as Unix milliseconds.
    pub authenticated_at_ms: u64,
}

/// Non-secret audit identifiers supplied for a possible first binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardOidcAuditContext {
    /// Stable audit event identifier.
    pub id: String,
    /// Correlation identifier for the browser callback.
    pub request_id: String,
}

/// Durable generic OIDC attempt and identity-binding operations.
pub trait YardOidcRepository: Send + Sync {
    /// Stores one hashed authorization attempt after bounded housekeeping.
    ///
    /// # Errors
    ///
    /// Returns conflict for replayed durable material, validation for malformed input, or a
    /// provider failure.
    fn create_yard_oidc_attempt(&self, attempt: &NewYardOidcAttempt)
    -> Result<(), RepositoryError>;

    /// Atomically claims one active state digest after bounded housekeeping.
    ///
    /// # Errors
    ///
    /// Returns concealed not-found for an unknown, expired, or claimed digest, validation for
    /// malformed input, or a provider failure.
    fn claim_yard_oidc_attempt(
        &self,
        state_hash: &str,
        now_ms: u64,
    ) -> Result<YardOidcAttemptRecord, RepositoryError>;

    /// Resolves or creates one exact workspace-scoped binding without provisioning authority.
    ///
    /// Email drift revokes the bound subject's active Yard sessions in the same transaction and
    /// returns concealed not-found. A first binding emits exactly one linked audit event.
    ///
    /// # Errors
    ///
    /// Returns concealed not-found for zero, ambiguous, inactive, foreign, or drifting authority,
    /// validation for malformed input, conflict for duplicate durable material, or a provider
    /// failure.
    fn authenticate_yard_oidc_identity(
        &self,
        authentication: &NewYardOidcAuthentication,
        audit: &YardOidcAuditContext,
    ) -> Result<YardOidcIdentityRecord, RepositoryError>;
}

/// Builds the exact redaction-safe audit metadata for a new OIDC identity binding.
#[must_use]
pub fn yard_oidc_identity_audit_metadata(
    yard_id: &str,
    subject_id: &str,
) -> Vec<(String, AuditValue)> {
    vec![
        (
            "subjectId".to_owned(),
            AuditValue::String(subject_id.to_owned()),
        ),
        ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        is_valid_oidc_provider_subject, normalize_oidc_email, normalize_oidc_issuer,
        yard_oidc_identity_audit_metadata,
    };
    use crate::AuditValue;

    #[test]
    fn oidc_link_audit_metadata_is_exact_and_secret_free() {
        assert_eq!(
            yard_oidc_identity_audit_metadata("yard_one", "user_one"),
            vec![
                (
                    "subjectId".to_owned(),
                    AuditValue::String("user_one".to_owned()),
                ),
                (
                    "yardId".to_owned(),
                    AuditValue::String("yard_one".to_owned()),
                ),
            ]
        );
    }

    #[test]
    fn issuer_normalization_requires_secure_or_loopback_origins() {
        for valid in [
            "https://identity.example.test",
            "https://identity.example.test/realms/core",
            "http://localhost:9000",
            "http://127.0.0.1:9000",
            "http://[::1]:9000",
        ] {
            assert!(normalize_oidc_issuer(valid).is_some(), "{valid}");
        }
        for invalid in [
            "",
            "file:///tmp/issuer",
            "http://identity.example.test",
            "https://user@identity.example.test",
            "https://identity.example.test?tenant=one",
            "https://identity.example.test#fragment",
        ] {
            assert_eq!(normalize_oidc_issuer(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn email_and_subject_validation_are_bounded_and_deterministic() {
        assert_eq!(
            normalize_oidc_email("  PERSON@Example.Test  "),
            Some("person@example.test".to_owned())
        );
        for invalid in ["", "person", "a@b@c", "person @example.test"] {
            assert_eq!(normalize_oidc_email(invalid), None, "{invalid}");
        }
        assert!(is_valid_oidc_provider_subject("provider-subject"));
        assert!(!is_valid_oidc_provider_subject(""));
        assert!(!is_valid_oidc_provider_subject("provider\nsubject"));
        assert!(!is_valid_oidc_provider_subject(&"s".repeat(513)));
    }
}
