#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{Arguments, run_command};
use clap::Parser;

#[tokio::test]
async fn serve_rejects_partial_oidc_before_binding() {
    let arguments = Arguments::try_parse_from([
        "blobyard-server",
        "serve",
        "--listen",
        "127.0.0.1:0",
        "--oidc-issuer",
        "https://identity.example.test/",
    ])
    .expect("serve arguments");

    assert!(run_command(arguments.command).await.is_err());
}
