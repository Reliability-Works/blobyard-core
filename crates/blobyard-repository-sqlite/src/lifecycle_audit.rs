use super::{map_error, rows};
use blobyard_contract::{AuditEventRecord, AuditPage, AuditValue, NewAuditEvent, RepositoryError};
use rusqlite::{Connection, Row, Statement, params};
use std::collections::BTreeMap;

pub(super) fn insert(
    connection: &Connection,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    validate(event)?;
    let metadata = encode_metadata(&event.metadata);
    connection
        .execute(
            "INSERT INTO audit_events (id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.id,
                event.workspace_id,
                event.actor,
                event.action,
                event.request_id,
                event.target_type,
                metadata,
                to_i64(event.created_at_ms)?,
            ],
        )
        .map(|_count| ())
        .map_err(map_error)
}

pub(super) fn list(
    connection: &Connection,
    workspace_id: &str,
    before: Option<u64>,
    limit: u32,
) -> Result<AuditPage, RepositoryError> {
    rows::validate_text(workspace_id)?;
    if !(1..=100).contains(&limit) {
        return Err(RepositoryError::InvalidInput);
    }
    let before = before.map(to_i64).transpose()?;
    let fetch = i64::from(limit) + 1;
    let mut statement = connection
        .prepare(
            "SELECT sequence, id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms FROM audit_events WHERE workspace_id = ?1 AND (?2 IS NULL OR sequence < ?2) ORDER BY sequence DESC LIMIT ?3",
        )
        .map_err(map_error)?;
    let mut items = query_audit(&mut statement, workspace_id, before, fetch)?;
    let has_more = (0_u32..).zip(&items).any(|(index, _item)| index == limit);
    if has_more {
        items.pop();
    }
    let next_before = has_more
        .then(|| items.last().map(|event| event.sequence))
        .flatten();
    Ok(AuditPage { items, next_before })
}

fn query_audit(
    statement: &mut Statement<'_>,
    workspace_id: &str,
    before: Option<i64>,
    fetch: i64,
) -> Result<Vec<AuditEventRecord>, RepositoryError> {
    statement
        .query_map(params![workspace_id, before, fetch], audit_row)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn validate(event: &NewAuditEvent) -> Result<(), RepositoryError> {
    for value in [
        &event.id,
        &event.workspace_id,
        &event.actor,
        &event.action,
        &event.request_id,
        &event.target_type,
    ] {
        rows::validate_text(value)?;
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, value) in &event.metadata {
        rows::validate_text(name)?;
        if !names.insert(name) {
            return Err(RepositoryError::InvalidInput);
        }
        if let AuditValue::String(text) = value {
            rows::validate_text(text)?;
        }
    }
    Ok(())
}

fn encode_metadata(values: &[(String, AuditValue)]) -> String {
    let map = values
        .iter()
        .map(|(name, value)| (name.clone(), json_value(value)))
        .collect::<BTreeMap<_, _>>();
    serde_json::Value::Object(map.into_iter().collect()).to_string()
}

fn json_value(value: &AuditValue) -> serde_json::Value {
    match value {
        AuditValue::String(value) => serde_json::Value::String(value.clone()),
        AuditValue::Number(value) => serde_json::Value::from(*value),
        AuditValue::Boolean(value) => serde_json::Value::from(*value),
        AuditValue::Null => serde_json::Value::Null,
    }
}

fn audit_row(row: &Row<'_>) -> rusqlite::Result<AuditEventRecord> {
    let sequence = required_u64(row.get(0)?)?;
    let metadata: String = row.get(7)?;
    Ok(AuditEventRecord {
        sequence,
        id: row.get(1)?,
        workspace_id: row.get(2)?,
        actor: row.get(3)?,
        action: row.get(4)?,
        request_id: row.get(5)?,
        target_type: row.get(6)?,
        metadata: decode_metadata(&metadata)?,
        created_at_ms: required_u64(row.get(8)?)?,
    })
}

fn decode_metadata(value: &str) -> rusqlite::Result<Vec<(String, AuditValue)>> {
    let map = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(value)
        .map_err(conversion_error)?;
    map.into_iter()
        .map(|(name, value)| decode_value(value).map(|decoded| (name, decoded)))
        .collect()
}

fn decode_value(value: serde_json::Value) -> rusqlite::Result<AuditValue> {
    match value {
        serde_json::Value::String(value) => Ok(AuditValue::String(value)),
        serde_json::Value::Number(value) => value
            .as_u64()
            .map(AuditValue::Number)
            .ok_or_else(|| conversion_error(value)),
        serde_json::Value::Bool(value) => Ok(AuditValue::Boolean(value)),
        serde_json::Value::Null => Ok(AuditValue::Null),
        other => Err(conversion_error(other)),
    }
}

pub(super) fn to_i64(value: u64) -> Result<i64, RepositoryError> {
    i64::try_from(value).map_err(|_error| RepositoryError::InvalidInput)
}

fn required_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(conversion_error)
}

fn conversion_error(value: impl std::fmt::Debug + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{value:?}"),
        )),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{audit_row, decode_metadata, query_audit, required_u64, to_i64};
    use blobyard_contract::{AuditValue, RepositoryError};
    use rusqlite::Connection;

    #[test]
    fn metadata_decoder_accepts_safe_scalars_and_rejects_unsafe_shapes() {
        assert_eq!(
            decode_metadata(r#"{"bool":true,"null":null,"number":7,"text":"safe"}"#)
                .expect("safe metadata"),
            vec![
                ("bool".to_owned(), AuditValue::Boolean(true)),
                ("null".to_owned(), AuditValue::Null),
                ("number".to_owned(), AuditValue::Number(7)),
                ("text".to_owned(), AuditValue::String("safe".to_owned())),
            ]
        );
        assert!(decode_metadata(r#"{"array":[]}"#).is_err());
        assert!(decode_metadata(r#"{"negative":-1}"#).is_err());
        assert!(decode_metadata("not-json").is_err());
    }

    #[test]
    fn persisted_integer_conversions_fail_closed() {
        assert_eq!(to_i64(u64::MAX), Err(RepositoryError::InvalidInput));
        assert!(required_u64(-1).is_err());
    }

    #[test]
    fn audit_rows_reject_each_malformed_provider_field() {
        let connection = Connection::open_in_memory().expect("connection");
        for query in [
            "SELECT 'bad', 'id', 'workspace', 'actor', 'action', 'request', 'target', '{}', 1",
            "SELECT 1, 2, 'workspace', 'actor', 'action', 'request', 'target', '{}', 1",
            "SELECT 1, 'id', 2, 'actor', 'action', 'request', 'target', '{}', 1",
            "SELECT 1, 'id', 'workspace', 2, 'action', 'request', 'target', '{}', 1",
            "SELECT 1, 'id', 'workspace', 'actor', 2, 'request', 'target', '{}', 1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 2, 'target', '{}', 1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 'request', 2, '{}', 1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 'request', 'target', 2, 1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 'request', 'target', '{}', 'bad'",
            "SELECT -1, 'id', 'workspace', 'actor', 'action', 'request', 'target', '{}', 1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 'request', 'target', '{}', -1",
            "SELECT 1, 'id', 'workspace', 'actor', 'action', 'request', 'target', 'bad', 1",
        ] {
            assert!(
                connection.query_row(query, [], audit_row).is_err(),
                "{query}"
            );
        }
    }

    #[test]
    fn audit_query_maps_parameter_failure() {
        let connection = Connection::open_in_memory().expect("connection");
        let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
        assert_eq!(
            query_audit(&mut statement, "workspace", None, 1).err(),
            Some(RepositoryError::Unavailable)
        );
    }
}
