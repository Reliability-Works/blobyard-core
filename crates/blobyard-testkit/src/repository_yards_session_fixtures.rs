use super::YardConformanceRepository;
use super::fixtures::{revoked_event, visibility_event};
use crate::hash;
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardStartRecord, YardVisibility,
};

pub(super) fn production_environment(
    mut environments: Vec<blobyard_contract::YardEnvironmentRecord>,
) -> Result<blobyard_contract::YardEnvironmentRecord, RepositoryError> {
    environments.pop().ok_or(RepositoryError::Unavailable)
}

pub(super) fn issue_session(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    suffix: &str,
    hash_character: char,
    at: u64,
) -> Result<blobyard_contract::YardSessionRecord, RepositoryError> {
    let admission =
        repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", at)?;
    let continuation = NewYardContinuation {
        id: format!("yardcont_{suffix}"),
        continuation_hash: hash(hash_character),
        code_hash: hash(previous_character(hash_character)),
        yard_id: admission.yard_id,
        environment_id: admission.environment_id,
        host_label: first.yard.host_label.clone(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: at,
        expires_at_ms: at + YARD_EXCHANGE_CODE_LIFETIME_MS,
    };
    repository.issue_yard_exchange_code(&continuation)?;
    repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &first.yard.host_label,
            &new_session(&format!("yardsession_{suffix}"), hash_character, at + 1),
            &YardSessionAuditContext {
                id: format!("audit_session_{suffix}"),
                request_id: format!("request_session_{suffix}"),
            },
            at + 1,
        )
        .map(|exchange| exchange.session)
}

pub(super) fn new_session(id: &str, hash_character: char, at: u64) -> NewYardSession {
    NewYardSession {
        id: id.to_owned(),
        token_hash: hash(hash_character),
        created_at_ms: at,
        expires_at_ms: at + YARD_SESSION_LIFETIME_MS,
    }
}

pub(super) fn set_visibility(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
    from: &str,
    to: YardVisibility,
    at: u64,
) -> Result<(), RepositoryError> {
    repository
        .set_yard_visibility(
            yard_id,
            to,
            at,
            &visibility_event(yard_id, from, to.as_str(), at),
        )
        .map(|_record| ())
}

pub(super) fn session_revoked_event(yard_id: &str, session_id: &str, at: u64) -> NewAuditEvent {
    let mut event = revoked_event(yard_id, session_id, at);
    "yard.session_revoked".clone_into(&mut event.action);
    "yard_session".clone_into(&mut event.target_type);
    event.metadata = vec![
        (
            "sessionId".to_owned(),
            AuditValue::String(session_id.to_owned()),
        ),
        ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
    ];
    event
}

const fn previous_character(value: char) -> char {
    match value {
        'e' => 'd',
        'f' => 'e',
        _ => '0',
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
mod tests {
    use super::{new_session, production_environment};
    use crate::hash;

    #[test]
    fn session_fixture_helpers_cover_normative_and_fallback_values() {
        assert_eq!(super::previous_character('7'), '0');
        let session = new_session("session_fixture", 'a', 10);
        assert_eq!(session.id, "session_fixture");
        assert_eq!(session.token_hash, hash('a'));
        assert_eq!(session.created_at_ms, 10);
        assert_eq!(
            session.expires_at_ms,
            10 + blobyard_contract::YARD_SESSION_LIFETIME_MS
        );
        assert_eq!(
            production_environment(Vec::new()),
            Err(blobyard_contract::RepositoryError::Unavailable)
        );
        let environment = blobyard_contract::YardEnvironmentRecord {
            id: "environment_fixture".to_owned(),
            yard_id: "yard_fixture".to_owned(),
            name: blobyard_core::Slug::new("production").expect("slug"),
            kind: blobyard_contract::YardEnvironmentKind::Production,
            status: blobyard_contract::YardEnvironmentStatus::Active,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        assert_eq!(
            production_environment(vec![environment.clone()]),
            Ok(environment)
        );
    }
}
