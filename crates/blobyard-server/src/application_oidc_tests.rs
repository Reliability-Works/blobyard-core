#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::serve_until_with_storage_and_oidc;
use crate::{StorageConfiguration, YardOidcConfiguration};
use blobyard_core::SecretString;
use tempfile::TempDir;

async fn unroutable_configuration() -> YardOidcConfiguration {
    let unused = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused provider listener");
    let issuer = format!(
        "http://{}/",
        unused.local_addr().expect("unused provider address")
    );
    drop(unused);
    YardOidcConfiguration::from_optional(
        Some(issuer),
        Some("fixture-client".to_owned()),
        SecretString::new("fixture-secret").ok(),
    )
    .expect("configuration")
    .expect("enabled OIDC")
}

async fn serve_failure(public_origin: &str) -> Result<(), Box<dyn std::error::Error>> {
    let configuration = unroutable_configuration().await;
    serve_until_with_storage_and_oidc(
        "127.0.0.1:0".parse().expect("loopback address"),
        TempDir::new().expect("root").path(),
        Some(public_origin),
        None,
        &StorageConfiguration::Filesystem,
        Some(&configuration),
        Box::pin(async {}),
    )
    .await
}

#[tokio::test]
async fn standalone_oidc_discovery_failure_precedes_listener_binding() {
    assert!(serve_failure("http://127.0.0.1:8787").await.is_err());
}

#[tokio::test]
async fn standalone_oidc_callback_transport_precedes_discovery_and_listener_binding() {
    let failure = serve_failure("http://core.example.test")
        .await
        .expect_err("a non-loopback HTTP public origin cannot carry OIDC callbacks");
    assert_eq!(
        failure.downcast_ref::<crate::ServerError>(),
        Some(&crate::ServerError::PublicOrigin),
        "callback transport is rejected before discovery or listener availability"
    );
}
