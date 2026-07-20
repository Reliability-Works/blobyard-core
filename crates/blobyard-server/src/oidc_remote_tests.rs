#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::RemoteGithubOidcVerifier;

#[test]
fn default_verifier_is_pinned_to_github() {
    let verifier = RemoteGithubOidcVerifier::default();
    assert_eq!(
        verifier.jwks_url.as_ref(),
        "https://token.actions.githubusercontent.com/.well-known/jwks"
    );
}
