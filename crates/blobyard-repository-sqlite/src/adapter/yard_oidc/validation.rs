use super::{auth_validation, repository_rows as rows, yard_guest_rows, yard_session_rows};
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YARD_OIDC_ATTEMPT_LIFETIME_MS,
    YardOidcAuditContext, is_valid_oidc_provider_subject, normalize_oidc_email,
    normalize_oidc_issuer,
};

pub(super) fn attempt(value: &NewYardOidcAttempt) -> Result<(), RepositoryError> {
    auth_validation::validate_hash(&value.state_hash)?;
    auth_validation::validate_hash(&value.continuation_hash)?;
    yard_session_rows::validate_host_label(&value.host_label)?;
    yard_session_rows::validate_return_path(&value.return_path)?;
    let expected = value
        .created_at_ms
        .saturating_add(YARD_OIDC_ATTEMPT_LIFETIME_MS);
    if expected == value.expires_at_ms {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn authentication(
    value: &NewYardOidcAuthentication,
    audit: &YardOidcAuditContext,
) -> Result<(), RepositoryError> {
    rows::validate_text(&audit.id)?;
    rows::validate_text(&audit.request_id)?;
    yard_session_rows::validate_host_label(&value.host_label)?;
    let issuer = normalize_oidc_issuer(&value.issuer);
    let email_valid = value.normalized_email.as_deref().is_none_or(|email| {
        normalize_oidc_email(email).as_deref() == Some(email)
            && yard_guest_rows::normalized_email(email)
    });
    if issuer.as_deref() == Some(value.issuer.as_str())
        && email_valid
        && is_valid_oidc_provider_subject(&value.provider_subject)
    {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn identity(
    issuer: &str,
    provider_subject: &str,
    normalized_email: &str,
    created_at_ms: u64,
    last_authenticated_at_ms: u64,
) -> Result<(), RepositoryError> {
    let issuer_valid = normalize_oidc_issuer(issuer).as_deref() == Some(issuer);
    let email_valid = normalize_oidc_email(normalized_email).as_deref() == Some(normalized_email);
    if issuer_valid
        && email_valid
        && is_valid_oidc_provider_subject(provider_subject)
        && last_authenticated_at_ms >= created_at_ms
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::{attempt, authentication, identity};
    use blobyard_contract::{
        NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError,
        YARD_OIDC_ATTEMPT_LIFETIME_MS, YardOidcAuditContext,
    };

    fn valid_attempt() -> NewYardOidcAttempt {
        NewYardOidcAttempt {
            state_hash: "a".repeat(64),
            continuation_hash: "b".repeat(64),
            host_label: "yard-123456789-fixture".to_owned(),
            return_path: "/reports".to_owned(),
            created_at_ms: 10,
            expires_at_ms: 10 + YARD_OIDC_ATTEMPT_LIFETIME_MS,
        }
    }

    fn valid_authentication() -> NewYardOidcAuthentication {
        NewYardOidcAuthentication {
            issuer: "https://identity.example.test/".to_owned(),
            provider_subject: "provider-subject".to_owned(),
            normalized_email: Some("person@example.test".to_owned()),
            host_label: "yard-123456789-fixture".to_owned(),
            authenticated_at_ms: 20,
        }
    }

    #[test]
    fn attempt_validation_rejects_each_invalid_boundary() {
        assert_eq!(attempt(&valid_attempt()), Ok(()));
        for invalid in [
            NewYardOidcAttempt {
                state_hash: "short".to_owned(),
                ..valid_attempt()
            },
            NewYardOidcAttempt {
                continuation_hash: "short".to_owned(),
                ..valid_attempt()
            },
            NewYardOidcAttempt {
                host_label: "invalid".to_owned(),
                ..valid_attempt()
            },
            NewYardOidcAttempt {
                return_path: "//foreign".to_owned(),
                ..valid_attempt()
            },
            NewYardOidcAttempt {
                expires_at_ms: 11,
                ..valid_attempt()
            },
        ] {
            assert_eq!(attempt(&invalid), Err(RepositoryError::InvalidInput));
        }
    }

    #[test]
    fn authentication_validation_fails_closed() {
        let audit = YardOidcAuditContext {
            id: "audit_oidc".to_owned(),
            request_id: "request_oidc".to_owned(),
        };
        assert_eq!(authentication(&valid_authentication(), &audit), Ok(()));
        for invalid in [
            NewYardOidcAuthentication {
                issuer: "http://identity.example.test/".to_owned(),
                ..valid_authentication()
            },
            NewYardOidcAuthentication {
                provider_subject: String::new(),
                ..valid_authentication()
            },
            NewYardOidcAuthentication {
                normalized_email: Some("Person@example.test".to_owned()),
                ..valid_authentication()
            },
            NewYardOidcAuthentication {
                host_label: "invalid".to_owned(),
                ..valid_authentication()
            },
        ] {
            assert_eq!(
                authentication(&invalid, &audit),
                Err(RepositoryError::InvalidInput)
            );
        }
        let bad_audit = YardOidcAuditContext {
            id: String::new(),
            ..audit.clone()
        };
        assert_eq!(
            authentication(&valid_authentication(), &bad_audit),
            Err(RepositoryError::InvalidInput)
        );
        let bad_request_id = YardOidcAuditContext {
            request_id: String::new(),
            ..audit.clone()
        };
        assert_eq!(
            authentication(&valid_authentication(), &bad_request_id),
            Err(RepositoryError::InvalidInput)
        );
        assert_eq!(
            authentication(
                &NewYardOidcAuthentication {
                    normalized_email: None,
                    ..valid_authentication()
                },
                &audit
            ),
            Ok(())
        );
    }

    #[test]
    fn durable_identity_validation_fails_closed() {
        assert_eq!(
            identity(
                "https://identity.example.test/",
                "subject",
                "person@example.test",
                10,
                10
            ),
            Ok(())
        );
        assert_eq!(
            identity(
                "https://identity.example.test/",
                "subject",
                "person@example.test",
                11,
                10
            ),
            Err(RepositoryError::Unavailable)
        );
    }
}
