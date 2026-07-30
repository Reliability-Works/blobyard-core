use super::{repository_rows, validation as yard_oidc_validation, yard_rows};
use blobyard_contract::{NewYardOidcAttempt, YardOidcAttemptRecord, YardOidcIdentityRecord};
use rusqlite::Row;

pub(super) const ATTEMPT_COLUMNS: &str = "state_hash, continuation_hash, host_label, return_path, created_at_ms, expires_at_ms, claimed_at_ms";
pub(super) const IDENTITY_COLUMNS: &str = "issuer, provider_subject, workspace_id, yard_subject_id, normalized_email, created_at_ms, last_authenticated_at_ms";

pub(super) fn attempt(row: &Row<'_>) -> rusqlite::Result<YardOidcAttemptRecord> {
    let record = YardOidcAttemptRecord {
        attempt: NewYardOidcAttempt {
            state_hash: row.get(0)?,
            continuation_hash: row.get(1)?,
            host_label: row.get(2)?,
            return_path: row.get(3)?,
            created_at_ms: yard_rows::required_u64(row.get(4)?)?,
            expires_at_ms: yard_rows::required_u64(row.get(5)?)?,
        },
        claimed_at_ms: yard_rows::optional_u64(row.get(6)?)?,
    };
    validate_attempt(&record).or(Err(repository_rows::conversion_error(
        record.attempt.state_hash.clone(),
    )))?;
    Ok(record)
}

pub(super) fn identity(row: &Row<'_>) -> rusqlite::Result<YardOidcIdentityRecord> {
    let record = YardOidcIdentityRecord {
        issuer: row.get(0)?,
        provider_subject: row.get(1)?,
        workspace_id: row.get(2)?,
        yard_subject_id: row.get(3)?,
        normalized_email: row.get(4)?,
        created_at_ms: yard_rows::required_u64(row.get(5)?)?,
        last_authenticated_at_ms: yard_rows::required_u64(row.get(6)?)?,
    };
    repository_rows::validate_text(&record.workspace_id)
        .and_then(|()| repository_rows::validate_text(&record.yard_subject_id))
        .and_then(|()| {
            yard_oidc_validation::identity(
                &record.issuer,
                &record.provider_subject,
                &record.normalized_email,
                record.created_at_ms,
                record.last_authenticated_at_ms,
            )
        })
        .or(Err(repository_rows::conversion_error(
            record.provider_subject.clone(),
        )))?;
    Ok(record)
}

fn validate_attempt(
    record: &YardOidcAttemptRecord,
) -> Result<(), blobyard_contract::RepositoryError> {
    yard_oidc_validation::attempt(&record.attempt)?;
    if record.claimed_at_ms.is_none_or(|claimed| {
        claimed >= record.attempt.created_at_ms && claimed < record.attempt.expires_at_ms
    }) {
        Ok(())
    } else {
        Err(blobyard_contract::RepositoryError::Unavailable)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
mod tests {
    use super::{attempt, identity};
    use rusqlite::Connection;

    #[test]
    fn row_decoders_reject_corrupt_provider_values() {
        let connection = Connection::open_in_memory().expect("connection");
        for query in [
            "SELECT 'a', 'b', 'yard-123456789-fixture', '/', 1, 600001, NULL",
            "SELECT 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'yard-123456789-fixture', '/', 1, 600001, 600001",
        ] {
            assert!(connection.query_row(query, [], attempt).is_err(), "{query}");
        }
        for query in [
            "SELECT 'http://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', '', 'workspace', 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', '', 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', '', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'Person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', 2, 1",
        ] {
            assert!(
                connection.query_row(query, [], identity).is_err(),
                "{query}"
            );
        }
    }

    #[test]
    fn row_decoders_reject_corrupt_provider_column_types() {
        let connection = Connection::open_in_memory().expect("connection");
        let state = "a".repeat(64);
        let continuation = "b".repeat(64);
        for query in [
            format!("SELECT 1, '{continuation}', 'yard-123456789-fixture', '/', 1, 600001, NULL"),
            format!("SELECT '{state}', 1, 'yard-123456789-fixture', '/', 1, 600001, NULL"),
            format!("SELECT '{state}', '{continuation}', 1, '/', 1, 600001, NULL"),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', 1, 1, 600001, NULL"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', 'time', 600001, NULL"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', -1, 599999, NULL"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', 1, 'time', NULL"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', 1, -1, NULL"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', 1, 600001, 'time'"
            ),
            format!(
                "SELECT '{state}', '{continuation}', 'yard-123456789-fixture', '/', 1, 600001, -1"
            ),
        ] {
            assert!(
                connection.query_row(&query, [], attempt).is_err(),
                "{query}"
            );
        }

        for query in [
            "SELECT 1, 'subject', 'workspace', 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 1, 'workspace', 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 1, 'user', 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 1, 'person@example.test', 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 1, 1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', 'time', 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', -1, 1",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', 1, 'time'",
            "SELECT 'https://identity.example.test/', 'subject', 'workspace', 'user', 'person@example.test', 1, -1",
        ] {
            assert!(
                connection.query_row(query, [], identity).is_err(),
                "{query}"
            );
        }
    }
}
