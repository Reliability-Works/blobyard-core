use super::*;
use crate::{Repository, repository_fault_tests::FaultingRepository};
use blobyard_api_client::{
    CreateInboxRequest, ListInboxesQuery, ResolveInboxQuery, RevokeInboxRequest,
};
use blobyard_contract::{InboxRecord, InboxStatus};
use blobyard_core::SecretString;
use std::{net::SocketAddr, sync::Arc};

fn inbox_record() -> InboxRecord {
    InboxRecord {
        id: "inbox".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: "Inbox".to_owned(),
        expires_at_ms: 1_000,
        status: InboxStatus::Active,
        current_files: 0,
        current_bytes: 0,
        reserved_files: 0,
        reserved_bytes: 0,
        maximum_files: 20,
        maximum_bytes: 100,
        created_at_ms: 1,
        revoked_at_ms: None,
    }
}

#[test]
fn inbox_contracts_fail_closed_for_unsafe_expiry_url_and_peer_values() {
    assert_eq!(
        contracts::expiry(1, None).expect("default expiry"),
        1 + 7 * 24 * 60 * 60 * 1_000
    );
    assert!(contracts::expiry(1, Some("31d")).is_err());
    assert!(contracts::expiry(u64::MAX, None).is_err());
    let token = SecretString::new("byin_fixture").expect("token");
    assert_eq!(
        contracts::inbox_url("https://example.com", &token)
            .expect("URL")
            .expose_secret(),
        "https://example.com/i/byin_fixture"
    );
    assert!(contracts::inbox_url("bad\norigin", &token).is_err());
    assert_eq!(contracts::peer_fingerprint(None), "unavailable");
    assert_eq!(
        contracts::peer_fingerprint(Some("127.0.0.1:8787".parse::<SocketAddr>().expect("peer"))),
        "127.0.0.1"
    );
    assert_eq!(contracts::resolve_rate_key("token", "peer").len(), 64);
}

#[test]
fn inbox_contracts_fail_closed_for_corrupt_capacity_and_expiry_values() {
    let mut overflow = inbox_record();
    overflow.current_files = u64::MAX;
    overflow.reserved_files = 1;
    assert_eq!(
        error_status(contracts::metadata(overflow)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut byte_overflow = inbox_record();
    byte_overflow.current_bytes = u64::MAX;
    byte_overflow.reserved_bytes = 1;
    assert_eq!(
        error_status(contracts::metadata(byte_overflow)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut oversized = inbox_record();
    oversized.maximum_files = u64::MAX;
    assert_eq!(
        error_status(contracts::metadata(oversized)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid_expiry = inbox_record();
    invalid_expiry.expires_at_ms = u64::MAX;
    assert_eq!(
        error_status(contracts::summary(invalid_expiry.clone())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(contracts::metadata(invalid_expiry)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn create_preparation_propagates_binding_clock_and_origin_failures() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let principal = Principal(fixture.principal.clone());
    let request = CreateInboxRequest {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
        name: "Inbox".to_owned(),
        expires: Some("1h".to_owned()),
    };
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &request,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut bound = principal.clone();
    bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &bound,
            &request,
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );
    let mut invalid_origin = fixture.state;
    invalid_origin.public_origin = "bad\norigin".to_owned();
    assert_eq!(
        error_status(operations::create_at(
            &invalid_origin,
            &principal,
            &request,
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn other_inbox_operations_propagate_clock_and_repository_failures() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let principal = Principal(fixture.principal.clone());
    let mut failed = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&failed.repository);
    failed.repository = Arc::new(FaultingRepository::new(inner, 0));
    let query = ResolveInboxQuery {
        token: SecretString::new("token").expect("token"),
    };
    assert_eq!(
        error_status(operations::resolve_at(&failed, &query, Ok(1), "peer")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::resolve_at(
            &fixture.state,
            &query,
            Err(ApiError::internal()),
            "peer",
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::revoke_at(
            &fixture.state,
            &principal,
            &RevokeInboxRequest {
                inbox_id: "missing".to_owned(),
            },
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let query = ListInboxesQuery {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
        cursor: Some("next".to_owned()),
    };
    assert_eq!(
        error_status(operations::list_at(&fixture.state, &principal, &query)),
        StatusCode::BAD_REQUEST
    );

    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut revoke_failed = fixture.state.clone();
    revoke_failed.repository = Arc::new(FaultingRepository::new(inner, 0));
    assert_eq!(
        error_status(operations::revoke_at(
            &revoke_failed,
            &principal,
            &RevokeInboxRequest {
                inbox_id: "missing".to_owned(),
            },
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
