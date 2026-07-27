use super::{rows, yard_application_policy_encoding, yard_rows};
use blobyard_contract::YardApplicationPolicyRecord;
use blobyard_core::{
    ApplicationPolicyGraph, EffectiveApplicationPolicy, canonicalize_application_policy,
    valid_source_manifest_digest,
};
use rusqlite::Row;

pub(super) fn policy_row(row: &Row<'_>) -> rusqlite::Result<YardApplicationPolicyRecord> {
    let revision = yard_rows::required_u64(row.get(2)?)?;
    let source_manifest_digest: String = row.get(3)?;
    let policy_json: String = row.get(4)?;
    let effective_json: String = row.get(5)?;
    let policy: ApplicationPolicyGraph =
        serde_json::from_str(&policy_json).map_err(rows::conversion_error)?;
    let effective: EffectiveApplicationPolicy =
        serde_json::from_str(&effective_json).map_err(rows::conversion_error)?;
    let canonical =
        canonicalize_application_policy(policy.clone()).map_err(rows::conversion_error)?;
    let valid = revision > 0
        && valid_source_manifest_digest(&source_manifest_digest)
        && canonical.graph == policy
        && canonical.effective == effective
        && yard_application_policy_encoding::encode_graph(&policy) == policy_json
        && yard_application_policy_encoding::encode_effective(&effective) == effective_json;
    if !valid {
        return Err(rows::conversion_error(
            "invalid persisted application policy",
        ));
    }
    Ok(YardApplicationPolicyRecord {
        yard_id: row.get(0)?,
        workspace_id: row.get(1)?,
        revision,
        source_manifest_digest,
        policy,
        effective,
        approved_at_ms: yard_rows::required_u64(row.get(6)?)?,
        approved_by_principal: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::policy_row;
    use blobyard_contract::YardApplicationPolicyRecord;
    use rusqlite::{Connection, params_from_iter, types::Value};

    fn valid_values() -> Vec<Value> {
        vec![
            Value::Text("yard".to_owned()),
            Value::Text("workspace".to_owned()),
            Value::Integer(1),
            Value::Text("a".repeat(64)),
            Value::Text(r#"{"defaultRole":null,"roles":{}}"#.to_owned()),
            Value::Text(r#"{"effectiveRoles":{},"effectivePermissions":{}}"#.to_owned()),
            Value::Integer(1),
            Value::Text("actor".to_owned()),
        ]
    }

    fn decode(values: Vec<Value>) -> rusqlite::Result<YardApplicationPolicyRecord> {
        Connection::open_in_memory().expect("connection").query_row(
            "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8",
            params_from_iter(values),
            policy_row,
        )
    }

    #[test]
    fn policy_row_rejects_every_invalid_column_and_graph() {
        assert!(decode(valid_values()).is_ok());
        for (index, value) in [
            (0, Value::Integer(1)),
            (1, Value::Integer(1)),
            (2, Value::Text("bad".to_owned())),
            (2, Value::Integer(-1)),
            (3, Value::Integer(1)),
            (3, Value::Text("invalid".to_owned())),
            (4, Value::Integer(1)),
            (4, Value::Text("{".to_owned())),
            (
                4,
                Value::Text(r#"{"defaultRole":"missing","roles":{}}"#.to_owned()),
            ),
            (5, Value::Integer(1)),
            (5, Value::Text("{".to_owned())),
            (6, Value::Text("bad".to_owned())),
            (6, Value::Integer(-1)),
            (7, Value::Integer(1)),
        ] {
            let mut values = valid_values();
            values[index] = value;
            assert!(decode(values).is_err(), "column {index}");
        }
    }
}
