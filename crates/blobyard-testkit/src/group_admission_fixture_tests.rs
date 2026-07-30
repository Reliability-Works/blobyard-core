#![allow(
    clippy::expect_used,
    reason = "static fixture tests require explicit setup"
)]

use blobyard_contract::RepositoryError;
use serde_json::{Value, json};

#[test]
fn generated_group_admission_fixtures_have_valid_inventory() -> Result<(), RepositoryError> {
    super::group_admission_fixture_conformance()
}

#[test]
fn generated_oidc_fixture_has_valid_inventory() -> Result<(), RepositoryError> {
    super::oidc_fixture_conformance()
}

#[test]
fn oidc_fixture_rejects_inventory_and_owner_drift() {
    let document = super::fixture_document(super::OIDC_FIXTURE).expect("OIDC fixture");
    for malformed in ["{", "{}"] {
        assert_eq!(
            super::oidc_fixture_conformance_for(malformed),
            Err(RepositoryError::Unavailable)
        );
    }
    let mut wrong_count = document.clone();
    wrong_count["cases"]
        .as_array_mut()
        .expect("OIDC cases")
        .pop();
    assert_eq!(
        super::oidc_fixture_conformance_for(&wrong_count.to_string()),
        Err(RepositoryError::Unavailable)
    );

    let mut duplicate = document.clone();
    duplicate["cases"][1]["id"] = duplicate["cases"][0]["id"].clone();
    assert_eq!(
        super::oidc_fixture_conformance_for(&duplicate.to_string()),
        Err(RepositoryError::Unavailable)
    );

    let mut wrong_suite = document;
    wrong_suite["cases"][0]["conformanceSuite"] = json!("unknown-suite");
    assert_eq!(
        super::oidc_fixture_conformance_for(&wrong_suite.to_string()),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn exact_case_assertion_rejects_semantic_drift() {
    let input = json!({"surface": "continuation"});
    let expected = json!({"lifetimeMilliseconds": 600_000});
    assert_eq!(
        super::assert_group_admission_fixture_case(
            "continuation-lifetime-is-ten-minutes",
            "server",
            &input,
            &expected,
        ),
        Ok(())
    );
    assert_eq!(
        super::assert_group_admission_fixture_case(
            "continuation-lifetime-is-ten-minutes",
            "sqlite",
            &input,
            &expected,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        super::assert_group_admission_fixture_case(
            "continuation-lifetime-is-ten-minutes",
            "server",
            &json!({"surface": "session"}),
            &expected,
        ),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn malformed_fixture_structures_fail_closed() {
    for document in [json!({}), json!({"cases": {}})] {
        assert_unavailable(&super::members(&document, "cases"));
    }
    for document in [
        json!({"cases": [{}]}),
        json!({"cases": [{"id": 1}]}),
        json!({"cases": [{"id": "duplicate"}, {"id": "duplicate"}]}),
    ] {
        let members = super::members(&document, "cases").unwrap_or(&[]);
        assert_unavailable(&super::unique_ids(members));
    }
}

#[test]
fn full_fixture_validation_propagates_each_failure() {
    let yard_document = super::fixture_document(super::YARD_SESSION_FIXTURE).expect("yard fixture");
    assert_fixture_failure("{", super::AUTHORIZATION_FIXTURE);
    assert_fixture_failure("{}", super::AUTHORIZATION_FIXTURE);
    let mut wrong_count = yard_document.clone();
    let cases = wrong_count["cases"].as_array_mut().expect("yard cases");
    cases.pop();
    assert_fixture_failure(&wrong_count.to_string(), super::AUTHORIZATION_FIXTURE);

    let mut duplicate_case_id = yard_document.clone();
    let cases = duplicate_case_id["cases"]
        .as_array_mut()
        .expect("yard cases");
    cases[1]["id"] = cases[0]["id"].clone();
    assert_fixture_failure(&duplicate_case_id.to_string(), super::AUTHORIZATION_FIXTURE);

    let mut missing_owner = yard_document.clone();
    missing_owner["cases"][0]["conformanceOwner"] = Value::Null;
    assert_fixture_failure(&missing_owner.to_string(), super::AUTHORIZATION_FIXTURE);

    let mut unexpected_owner = yard_document.clone();
    unexpected_owner["cases"][0]["conformanceOwner"] = json!("browser");
    assert_fixture_failure(&unexpected_owner.to_string(), super::AUTHORIZATION_FIXTURE);

    let mut wrong_cloud_set = yard_document;
    let cases = wrong_cloud_set["cases"].as_array_mut().expect("yard cases");
    let cloud_case = cases
        .iter_mut()
        .find(|member| member["conformanceOwner"] == "cloud")
        .expect("cloud case");
    cloud_case["conformanceOwner"] = json!("server");
    cloud_case["conformanceSuite"] = json!("group-machine");
    assert_fixture_failure(&wrong_cloud_set.to_string(), super::AUTHORIZATION_FIXTURE);

    assert_fixture_failure(super::YARD_SESSION_FIXTURE, "{");
    assert_fixture_failure(super::YARD_SESSION_FIXTURE, "{}");

    let duplicate = json!({
        "vectors": [
            {"id": "duplicate"},
            {"id": "duplicate"}
        ]
    });
    assert_eq!(
        super::unique_member(&duplicate, "vectors", "duplicate"),
        Err(RepositoryError::Unavailable)
    );

    assert_eq!(
        super::assert_group_authorization_fixture_case(
            "cross-workspace-access-grant-is-concealed",
            &json!({}),
        ),
        Err(RepositoryError::Unavailable)
    );

    let duplicate_authorization = duplicate.to_string();
    assert_fixture_failure(super::YARD_SESSION_FIXTURE, &duplicate_authorization);
}

#[test]
fn fixture_member_helpers_propagate_malformed_inputs() {
    let authorization =
        super::fixture_document(super::AUTHORIZATION_FIXTURE).expect("authorization fixture");
    let exact = super::unique_member(
        &authorization,
        "vectors",
        "cross-workspace-access-grant-is-concealed",
    )
    .expect("authorization vector");
    assert_eq!(
        super::assert_group_authorization_fixture_case(
            "cross-workspace-access-grant-is-concealed",
            exact,
        ),
        Ok(())
    );

    for fixture in ["{", "{}"] {
        assert_eq!(
            super::assert_exact_fixture_member(fixture, "vectors", "missing", &json!({})),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            super::assert_fixture_member(
                fixture,
                "vectors",
                "missing",
                None,
                None,
                &Value::Null,
                &Value::Null,
            ),
            Err(RepositoryError::Unavailable)
        );
    }
    let empty_collection = json!({"vectors": []}).to_string();
    assert_eq!(
        super::assert_exact_fixture_member(&empty_collection, "vectors", "missing", &json!({}),),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        super::ensure_owners(&[json!({
            "id": "same-user-in-two-workspaces-does-not-bridge-authority",
            "conformanceOwner": "cloud"
        })]),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        super::ensure_owners(&[json!({
            "conformanceOwner": "cloud",
            "conformanceSuite": "hosted-admission"
        })]),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn execution_tracker_accumulates_record_failures_until_finish() {
    let input = json!({"surface": "continuation"});
    let expected = json!({"lifetimeMilliseconds": 600_000});

    let mut complete = super::FixtureExecutionTracker::new("server", "session-contracts");
    complete.record_case("continuation-lifetime-is-ten-minutes", &input, &expected);
    assert_eq!(complete.finish(), Ok(()));

    assert_eq!(
        super::FixtureExecutionTracker::new("server", "session-contracts").finish(),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        super::FixtureExecutionTracker::new("server", "unknown-suite").finish(),
        Err(RepositoryError::Unavailable)
    );

    for (owner, suite, id, asserted_input) in [
        (
            "server",
            "session-contracts",
            "continuation-lifetime-is-ten-minutes",
            json!({"surface": "session"}),
        ),
        (
            "server",
            "session-cookie",
            "continuation-lifetime-is-ten-minutes",
            input.clone(),
        ),
        ("server", "session-contracts", "unknown-case", input.clone()),
    ] {
        let mut tracker = super::FixtureExecutionTracker::new(owner, suite);
        tracker.record_case(id, &asserted_input, &expected);
        assert_eq!(tracker.finish(), Err(RepositoryError::Unavailable));
    }

    let mut duplicate = super::FixtureExecutionTracker::new("server", "session-contracts");
    duplicate.record_case("continuation-lifetime-is-ten-minutes", &input, &expected);
    duplicate.record_case("continuation-lifetime-is-ten-minutes", &input, &expected);
    assert_eq!(duplicate.finish(), Err(RepositoryError::Unavailable));
}

#[test]
fn execution_aggregate_rejects_each_malformed_fixture_shape() {
    let recorded = std::iter::once("fixture").map(str::to_owned).collect();
    for fixture in [
        "{",
        "{}",
        r#"{"cases":[{"conformanceOwner":"test","conformanceSuite":"suite"}]}"#,
    ] {
        assert_eq!(
            super::fixture_execution_conformance_for(fixture, "test", "suite", &recorded),
            Err(RepositoryError::Unavailable)
        );
    }
}

fn assert_unavailable<T>(result: &Result<T, RepositoryError>) {
    assert_eq!(result.as_ref().err(), Some(&RepositoryError::Unavailable));
}

fn assert_fixture_failure(yard_fixture: &str, authorization_fixture: &str) {
    assert_eq!(
        super::group_admission_fixture_conformance_for(yard_fixture, authorization_fixture),
        Err(RepositoryError::Unavailable)
    );
}
