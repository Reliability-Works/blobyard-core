#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    IdentityBase, authority, effective_application_authority, identity_base,
    require_admitted_session,
};
use blobyard_contract::{RepositoryError, YardApplicationPolicyRecord};
use blobyard_core::{ApplicationPolicyGraph, EffectiveApplicationPolicy};
use rusqlite::{Connection, params_from_iter, types::Value};
use std::collections::BTreeMap;

fn policy_with_effective_roles(
    effective_roles: BTreeMap<String, Vec<String>>,
) -> YardApplicationPolicyRecord {
    YardApplicationPolicyRecord {
        yard_id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        revision: 1,
        source_manifest_digest: "a".repeat(64),
        policy: ApplicationPolicyGraph {
            default_role: None,
            roles: BTreeMap::new(),
        },
        effective: EffectiveApplicationPolicy {
            effective_roles,
            effective_permissions: BTreeMap::new(),
        },
        approved_at_ms: 1,
        approved_by_principal: "fixture".to_owned(),
    }
}

#[test]
fn incomplete_effective_policy_does_not_grant_permissions() {
    let policy = policy_with_effective_roles(BTreeMap::from([(
        "viewer".to_owned(),
        vec!["viewer".to_owned()],
    )]));
    assert_eq!(
        effective_application_authority(Some(policy), vec!["viewer".to_owned()]),
        (vec!["viewer".to_owned()], Vec::new())
    );
    assert_eq!(
        effective_application_authority(
            Some(policy_with_effective_roles(BTreeMap::new())),
            vec!["missing".to_owned()],
        ),
        (Vec::new(), Vec::new())
    );
}

#[test]
fn identity_base_rejects_non_text_columns() {
    let valid = || {
        vec![
            Value::Text("session".to_owned()),
            Value::Text("user".to_owned()),
            Value::Text("member".to_owned()),
            Value::Null,
            Value::Text("workspace".to_owned()),
            Value::Text("project".to_owned()),
            Value::Text("yard".to_owned()),
            Value::Text("environment".to_owned()),
            Value::Text("Display".to_owned()),
            Value::Text("email@example.test".to_owned()),
            Value::Text("selected".to_owned()),
        ]
    };
    let decode = |values| {
        Connection::open_in_memory().expect("connection").query_row(
            "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11",
            params_from_iter(values),
            identity_base,
        )
    };
    assert!(decode(valid()).is_ok());
    for index in 0..=10 {
        let mut values = valid();
        values[index] = Value::Integer(1);
        assert!(decode(values).is_err(), "column {index}");
    }
    let mut nullable = valid();
    nullable[3] = Value::Null;
    nullable[8] = Value::Null;
    nullable[9] = Value::Null;
    assert!(decode(nullable).is_ok());
}

#[test]
fn resolved_session_must_still_be_the_admitted_session() {
    assert_eq!(require_admitted_session(Some("session"), "session"), Ok(()));
    assert_eq!(
        require_admitted_session(None, "session"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        require_admitted_session(Some("other"), "session"),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn unknown_subject_kinds_fail_closed() {
    let base = IdentityBase {
        session_id: "session".to_owned(),
        user_id: "subject".to_owned(),
        subject_kind: "unknown".to_owned(),
        invitation_id: None,
        workspace_id: "workspace".to_owned(),
        project_id: "project".to_owned(),
        yard_id: "yard".to_owned(),
        environment_id: "environment".to_owned(),
        display_name: None,
        email: None,
        visibility: "selected".to_owned(),
    };
    assert!(matches!(
        authority(&Connection::open_in_memory().expect("connection"), &base, 1),
        Err(RepositoryError::Unavailable)
    ));
}
