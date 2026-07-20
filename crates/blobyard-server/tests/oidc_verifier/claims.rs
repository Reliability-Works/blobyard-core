use super::*;

#[test]
fn normal_library_claim_contract_accepts_supported_variants() {
    assert!(verify_test_claims(claims(), AUDIENCE, NOW_MS).is_ok());
    let mut valid_variants = Vec::new();
    let mut legacy_subject = claims();
    legacy_subject["sub"] = json!("repo:Reliability-Works/Blobyard-Core:ref:refs/heads/main");
    valid_variants.push(legacy_subject);
    let mut yaml_workflow = claims();
    yaml_workflow["workflow_ref"] =
        json!("Reliability-Works/Blobyard-Core/.github/workflows/release.yaml@refs/tags/v1.2.3");
    valid_variants.push(yaml_workflow);
    for git_ref in [
        "refs/tags/v1.2.3",
        "refs/pull/123/head",
        "refs/pull/123/merge",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let mut value = claims();
        value["ref"] = json!(git_ref);
        valid_variants.push(value);
    }
    let mut environment = claims();
    environment["environment"] = json!("Release Approval");
    environment["sub"] =
        json!("repo:Reliability-Works/Blobyard-Core:environment:Release%20Approval");
    valid_variants.push(environment);
    let mut claimed_environment = claims();
    claimed_environment["environment"] = json!("Release Approval");
    valid_variants.push(claimed_environment);
    let mut optional = claims();
    optional["run_attempt"] = json!("1");
    optional["sha"] = json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    valid_variants.push(optional);
    for value in valid_variants {
        assert!(verify_test_claims(value, AUDIENCE, NOW_MS).is_ok());
    }
}

#[test]
fn normal_library_claim_contract_rejects_invalid_fields_and_refs() {
    invalid_claim(json!({}));
    for (field, value) in [
        ("aud", json!("https://other.example")),
        ("iss", json!("https://issuer.example")),
        ("repository", json!("invalid")),
        ("repository", json!("owner_/repository")),
        ("repository", json!("owner/repository_")),
        ("repository", json!("owner/repository!")),
        ("repository_owner", json!("other")),
        ("repository_owner", json!("")),
        ("repository_owner", json!("owner\n")),
        ("run_id", json!("")),
        ("run_id", json!("x".repeat(101))),
        ("run_attempt", json!("x".repeat(21))),
        ("run_attempt", json!("\n")),
        ("sha", json!("A".repeat(40))),
        ("ref", json!("refs/heads/")),
        ("ref", json!("refs/heads/a..b")),
        ("ref", json!("refs/heads/bad!")),
        ("ref", json!("refs/pull/0/head")),
        ("ref", json!("refs/pull/01/head")),
        ("ref", json!("refs/pull/not-a-number/head")),
        ("ref", json!("refs/pull/123/invalid")),
        ("ref", json!("refs/pull/123")),
    ] {
        let mut variant = claims();
        variant[field] = value;
        invalid_claim(variant);
    }
}

#[test]
fn normal_library_claim_contract_rejects_invalid_subjects_and_workflows() {
    for subject in [
        "invalid",
        "repo:invalid:ref:refs/heads/main",
        "repo:Reliability-Works@123/Blobyard-Core:ref:refs/heads/main",
        "repo:Reliability-Works@invalid/Blobyard-Core@456:ref:refs/heads/main",
        "repo:Reliability-Works@/Blobyard-Core@456:ref:refs/heads/main",
        "repo:Reliability-Works@123/Blobyard-Core@invalid:ref:refs/heads/main",
        "repo:owner_/repository:ref:refs/heads/main",
        "repo:Reliability-Works/Blobyard-Core",
        "repo:Other/Repository:ref:refs/heads/main",
        "repo:Reliability-Works/Blobyard-Core:environment:%FF",
    ] {
        let mut variant = claims();
        variant["sub"] = json!(subject);
        invalid_claim(variant);
    }
    for workflow in [
        "missing-at",
        "Reliability-Works/Blobyard-Core/@refs/heads/main",
        "Other/Repository/.github/workflows/release.yml@refs/heads/main",
        "Reliability-Works/Blobyard-Core/release.yml@refs/heads/main",
        "Reliability-Works/Blobyard-Core/.github/workflows/release.txt@refs/heads/main",
        "Reliability-Works/Blobyard-Core/.github/workflows/nested/release.yml@refs/heads/main",
        "Reliability-Works/Blobyard-Core/.github/workflows/release.yml@invalid",
    ] {
        let mut variant = claims();
        variant["workflow_ref"] = json!(workflow);
        invalid_claim(variant);
    }
}

#[test]
fn normal_library_claim_contract_rejects_invalid_times_and_environments() {
    let mut invalid = Vec::new();
    let mut overflow = claims();
    overflow["exp"] = json!(u64::MAX);
    overflow["iat"] = json!(u64::MAX - 600);
    overflow["nbf"] = json!(u64::MAX - 600);
    invalid.push(overflow);
    let mut expired = claims();
    expired["exp"] = json!(NOW_MS / 1_000);
    invalid.push(expired);
    let mut old = claims();
    old["iat"] = json!(NOW_MS / 1_000 - 601);
    invalid.push(old);
    let mut future_iat = claims();
    future_iat["iat"] = json!(NOW_MS / 1_000 + 6);
    invalid.push(future_iat);
    let mut future_nbf = claims();
    future_nbf["nbf"] = json!(NOW_MS / 1_000 + 6);
    invalid.push(future_nbf);
    let mut invalid_window = claims();
    invalid_window["iat"] = json!(NOW_MS / 1_000 - 901);
    invalid.push(invalid_window);
    let mut mismatched_environment = claims();
    mismatched_environment["environment"] = json!("production");
    mismatched_environment["sub"] =
        json!("repo:Reliability-Works/Blobyard-Core:environment:staging");
    invalid.push(mismatched_environment);
    let mut empty_environment = claims();
    empty_environment["environment"] = json!("");
    invalid.push(empty_environment);

    for value in invalid {
        invalid_claim(value);
    }
}
