#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use axum::{
    extract::ConnectInfo,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use blobyard_contract::RepositoryError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn status(error: ApiError) -> StatusCode {
    error.into_response().status()
}

#[test]
fn inbox_capabilities_accept_only_the_canonical_secret_shape() {
    assert!(valid_capability(&format!("byin_{}", "a".repeat(64))));
    for value in [
        format!("byin_{}", "a".repeat(63)),
        format!("byin_{}", "a".repeat(65)),
        format!("byin_{}", "A".repeat(64)),
        format!("byin_{}", "g".repeat(64)),
        "byin_ fixture".to_owned(),
        "fixture".to_owned(),
    ] {
        assert!(!valid_capability(&value), "accepted {value}");
    }
}

#[test]
fn inbox_rate_classes_and_repository_errors_fail_closed() {
    assert_eq!(RateKind::Upload.limits(), ("upload", 3_600_000, 20));
    assert_eq!(RateKind::Transfer.limits(), ("transfer", 60_000, 120));
    for error in [RepositoryError::NotFound, RepositoryError::InvalidInput] {
        assert_eq!(
            status(ApiError::concealed_capability(error)),
            StatusCode::NOT_FOUND
        );
    }
    for error in [
        RepositoryError::Conflict,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(
            status(ApiError::concealed_capability(error)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn inbox_authority_fingerprints_the_connected_peer() {
    let fixture = crate::transfers::test_seams::fixture(&["inbox:manage"]);
    let mut request = Request::new(());
    let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 44)), 443);
    request.extensions_mut().insert(ConnectInfo(peer));
    request
        .headers_mut()
        .insert(INBOX_HEADER, "malformed".parse().expect("header"));
    let (mut parts, _body) = request.into_parts();
    let authority = UploadAuthority::from_request_parts(&mut parts, &fixture.state)
        .await
        .expect("authority");
    let UploadAuthority::Inbox(credential) = authority else {
        unreachable!("expected inbox authority");
    };
    assert_eq!(
        credential.fingerprint_hash,
        hash("guest-fingerprint\x00192.0.2.44")
    );
}
