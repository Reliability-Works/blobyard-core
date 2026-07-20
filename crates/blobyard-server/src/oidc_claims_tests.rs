#![allow(clippy::expect_used, reason = "test assertions must fail loudly")]

use super::*;

const NOW_MS: u64 = 1_800_000_000_000;
const AUDIENCE: &str = "https://api.blobyard.local";

fn claims(subject: &str) -> GithubClaims {
    GithubClaims {
        aud: AUDIENCE.to_owned(),
        environment: None,
        exp: NOW_MS / 1_000 + 600,
        iat: NOW_MS / 1_000,
        iss: GITHUB_OIDC_ISSUER.to_owned(),
        nbf: NOW_MS / 1_000,
        git_ref: "refs/heads/main".to_owned(),
        repository: "Reliability-Works/Blobyard-Core".to_owned(),
        repository_owner: "Reliability-Works".to_owned(),
        run_attempt: Some("1".to_owned()),
        run_id: "12345".to_owned(),
        sha: Some("a".repeat(40)),
        sub: subject.to_owned(),
        workflow_ref:
            "Reliability-Works/Blobyard-Core/.github/workflows/release.yml@refs/heads/main"
                .to_owned(),
    }
}

fn assert_invalid_claims(value: GithubClaims) {
    assert_eq!(
        identity(value, AUDIENCE, NOW_MS),
        Err(OidcVerificationError::Invalid)
    );
}

#[test]
fn accepts_legacy_and_immutable_repository_subjects() {
    for subject in [
        "repo:Reliability-Works/Blobyard-Core:ref:refs/heads/main",
        "repo:Reliability-Works@123/Blobyard-Core@456:ref:refs/heads/main",
    ] {
        let identity = identity(claims(subject), AUDIENCE, NOW_MS).expect("identity");
        assert_eq!(identity.repository, "reliability-works/blobyard-core");
        assert_eq!(identity.workflow_path, ".github/workflows/release.yml");
        assert_eq!(identity.workflow_ref, "refs/heads/main");
        assert_eq!(
            identity.sha.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }
}

#[test]
fn derives_and_cross_checks_encoded_environment() {
    let subject = "repo:Reliability-Works/Blobyard-Core:environment:Release%20Approval";
    let derived = identity(claims(subject), AUDIENCE, NOW_MS).expect("derived environment");
    assert_eq!(derived.environment.as_deref(), Some("Release Approval"));

    let mut matched = claims(subject);
    matched.environment = Some("Release Approval".to_owned());
    assert!(identity(matched, AUDIENCE, NOW_MS).is_ok());
    let mut mismatched = claims(subject);
    mismatched.environment = Some("production".to_owned());
    assert_eq!(
        identity(mismatched, AUDIENCE, NOW_MS),
        Err(OidcVerificationError::Invalid)
    );
}

#[test]
fn rejects_mismatched_identity_and_invalid_time_windows() {
    let subject = "repo:Reliability-Works/Blobyard-Core:ref:refs/heads/main";
    let mut variants = Vec::new();
    let mut repository = claims(subject);
    repository.repository = "other/repository".to_owned();
    variants.push(repository);
    let mut owner = claims(subject);
    owner.repository_owner = "other".to_owned();
    variants.push(owner);
    let mut audience = claims(subject);
    audience.aud = "https://other.example".to_owned();
    variants.push(audience);
    let mut issuer = claims(subject);
    issuer.iss = "https://issuer.example".to_owned();
    variants.push(issuer);
    let mut expired = claims(subject);
    expired.exp = NOW_MS / 1_000;
    variants.push(expired);
    let mut old = claims(subject);
    old.iat -= 601;
    variants.push(old);
    let mut future = claims(subject);
    future.nbf += 6;
    variants.push(future);
    for variant in variants {
        assert_eq!(
            identity(variant, AUDIENCE, NOW_MS),
            Err(OidcVerificationError::Invalid)
        );
    }
}

#[test]
fn rejects_malformed_optional_and_workflow_claims() {
    let subject = "repo:Reliability-Works/Blobyard-Core:ref:refs/heads/main";
    let mut sha = claims(subject);
    sha.sha = Some("A".repeat(40));
    assert_invalid_claims(sha);
    let mut workflow = claims(subject);
    workflow.workflow_ref =
        "other/repository/.github/workflows/release.yml@refs/heads/main".to_owned();
    assert_invalid_claims(workflow);
    let mut attempt = claims(subject);
    attempt.run_attempt = Some("\n".to_owned());
    assert_invalid_claims(attempt);
    let mut run_id = claims(subject);
    run_id.run_id = String::new();
    assert_invalid_claims(run_id);
    let mixed_subject = "repo:Reliability-Works@123/Blobyard-Core:ref:refs/heads/main";
    assert_invalid_claims(claims(mixed_subject));
    let mut pull = claims(subject);
    pull.git_ref = "refs/pull/not-a-number/head".to_owned();
    assert_invalid_claims(pull);
    let mut incomplete_pull = claims(subject);
    incomplete_pull.git_ref = "refs/pull/123".to_owned();
    assert_invalid_claims(incomplete_pull);

    let mut subject_repository = claims(subject);
    subject_repository.repository = "other/repository".to_owned();
    subject_repository.repository_owner = "other".to_owned();
    assert_invalid_claims(subject_repository);
    for malformed_subject in [
        "repo:Reliability-Works@invalid/Blobyard-Core@456:ref:refs/heads/main",
        "repo:Reliability-Works@/Blobyard-Core@456:ref:refs/heads/main",
        "repo:Reliability-Works@123/Blobyard-Core@invalid:ref:refs/heads/main",
        "repo:owner_/repository:ref:refs/heads/main",
        "repo:Reliability-Works/Blobyard-Core",
    ] {
        assert_invalid_claims(claims(malformed_subject));
    }
    let mut repository_shape = claims(subject);
    repository_shape.repository = "Reliability-Works/.invalid".to_owned();
    assert_invalid_claims(repository_shape);
    let mut workflow_path = claims(subject);
    workflow_path.workflow_ref =
        "Reliability-Works/Blobyard-Core/release.yml@refs/heads/main".to_owned();
    assert_invalid_claims(workflow_path);
    let mut workflow_extension = claims(subject);
    workflow_extension.workflow_ref =
        "Reliability-Works/Blobyard-Core/.github/workflows/release.txt@refs/heads/main".to_owned();
    assert_invalid_claims(workflow_extension);
    for pull_ref in ["refs/pull/123", "refs/pull/123/invalid"] {
        let mut pull = claims(subject);
        pull.git_ref = pull_ref.to_owned();
        assert_invalid_claims(pull);
    }
}
