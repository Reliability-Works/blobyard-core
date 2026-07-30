#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{authenticate, scope};
use blobyard_contract::{NewYardOidcAuthentication, RepositoryError, YardOidcAuditContext};
use rusqlite::Connection;

fn authentication() -> NewYardOidcAuthentication {
    NewYardOidcAuthentication {
        issuer: "https://identity.example.test/".to_owned(),
        provider_subject: "provider-subject".to_owned(),
        normalized_email: Some("person@example.test".to_owned()),
        host_label: "yard-123456789-fixture".to_owned(),
        authenticated_at_ms: 1,
    }
}

fn audit() -> YardOidcAuditContext {
    YardOidcAuditContext {
        id: "audit_oidc".to_owned(),
        request_id: "request_oidc".to_owned(),
    }
}

#[test]
fn authentication_propagates_validation_and_time_failures() {
    let mut connection = Connection::open_in_memory().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        authenticate(
            &transaction,
            &NewYardOidcAuthentication {
                issuer: "http://identity.example.test/".to_owned(),
                ..authentication()
            },
            &audit(),
        )
        .err(),
        Some(RepositoryError::InvalidInput)
    );
    assert_eq!(
        authenticate(
            &transaction,
            &NewYardOidcAuthentication {
                authenticated_at_ms: (i64::MAX as u64) + 1,
                ..authentication()
            },
            &audit(),
        )
        .err(),
        Some(RepositoryError::InvalidInput)
    );
}

#[test]
fn scope_decoder_rejects_each_corrupt_selected_column() {
    for values in [
        "1, 'environment', 'workspace'",
        "'yard', 1, 'workspace'",
        "'yard', 'environment', 1",
    ] {
        let mut connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(&format!(
                "CREATE TABLE web_yards (
                   id, workspace_id, status, host_label
                 );
                 CREATE TABLE yard_environments (
                   id, yard_id, kind, status
                 );
                 CREATE TABLE yard_deploys (
                   yard_id, deployment_host_label, status
                 );
                 INSERT INTO web_yards
                   (id, workspace_id, status, host_label)
                 SELECT column1, column3, 'active', 'yard-123456789-fixture'
                 FROM (VALUES ({values}));
                 INSERT INTO yard_environments
                   (id, yard_id, kind, status)
                 SELECT column2, column1, 'production', 'active'
                 FROM (VALUES ({values}));"
            ))
            .expect("corrupt scope");
        let transaction = connection.transaction().expect("transaction");
        assert_eq!(
            scope(&transaction, "yard-123456789-fixture").err(),
            Some(RepositoryError::Unavailable)
        );
    }
}
