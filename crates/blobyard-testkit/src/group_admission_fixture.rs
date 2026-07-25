use crate::{groups::GROUP_CASE_IDS, repository::yard_session_case_ids};
use blobyard_contract::RepositoryError;
use serde_json::Value;
use std::collections::BTreeSet;

const YARD_SESSION_FIXTURE: &str = include_str!("../../../conformance/behavior/yard-sessions.json");
const AUTHORIZATION_FIXTURE: &str = include_str!("../../../conformance/authorization/vectors.json");

const SQLITE_CASE_IDS: &[&str] = &[
    "active-grant-with-revocation-timestamp-is-inert",
    "active-group-grant-limit-accepts-last-and-rejects-next",
    "active-group-limit-accepts-last-and-rejects-next",
    "active-group-with-deactivation-timestamp-is-inert",
    "active-member-limit-accepts-last-and-rejects-next",
    "active-membership-limit-accepts-last-and-rejects-next",
    "corrupt-group-membership-cardinality-is-inert",
    "environment-replacement-between-issue-and-exchange-denies",
    "group-pagination-is-deterministic-and-cursor-safe",
    "member-pagination-is-deterministic-and-cursor-safe",
    "nonmatching-environment-group-grant-denies",
    "unresolved-legacy-group-grant-remains-inert",
];

const SERVER_CASE_IDS: &[&str] = &[
    "machine-principal-is-denied-each-group-operation",
    "session-cookie-name-is-host-scoped",
];

const CLOUD_CASE_IDS: &[&str] = &[
    "same-user-in-two-workspaces-does-not-bridge-authority",
    "workspace-membership-removal-denies-on-next-private-request",
];

const GROUP_AUTHORIZATION_CASE_IDS: &[&str] = &[
    "cross-workspace-groups-are-concealed",
    "machine-cannot-add-workspace-group-member",
    "machine-cannot-create-workspace-group",
    "machine-cannot-deactivate-workspace-group",
    "machine-cannot-list-workspace-group-members",
    "machine-cannot-list-workspace-groups",
    "machine-cannot-remove-workspace-group-member",
    "machine-cannot-rename-workspace-group",
    "object-reader-cannot-manage-workspace-groups",
    "users-manager-can-manage-workspace-groups",
];

/// Verifies that generated group/admission fixtures have an exact execution owner.
///
/// Testkit-owned cases are exercised by `yard_conformance` or
/// `group_conformance`. `SQLite`- and server-owned cases are exercised by their
/// focused suites. Cloud-only membership cases are explicitly marked so Core
/// does not misrepresent Better Auth membership as local-user behavior.
///
/// # Errors
///
/// Returns unavailable if generated JSON cannot be parsed, contains duplicate
/// identifiers, or drifts from the execution-owner ledger.
pub fn group_admission_fixture_conformance() -> Result<(), RepositoryError> {
    group_admission_fixture_conformance_for(YARD_SESSION_FIXTURE, AUTHORIZATION_FIXTURE)
}

fn group_admission_fixture_conformance_for(
    yard_session_fixture: &str,
    authorization_fixture: &str,
) -> Result<(), RepositoryError> {
    let document = fixture_document(yard_session_fixture)?;
    let yard_session_ids = member_ids(&document, "cases")?;
    let owned_ids = yard_session_case_ids()
        .iter()
        .chain(GROUP_CASE_IDS)
        .chain(SQLITE_CASE_IDS)
        .chain(SERVER_CASE_IDS)
        .chain(CLOUD_CASE_IDS)
        .copied()
        .collect::<BTreeSet<_>>();
    ensure_exact(&yard_session_ids, &owned_ids)?;
    ensure_cloud_ownership(&document)?;

    let authorization = fixture_document(authorization_fixture)?;
    let group_authorization_ids = member_ids(&authorization, "vectors")?
        .into_iter()
        .filter(|id| {
            id.contains("workspace-group") || *id == "cross-workspace-groups-are-concealed"
        })
        .collect::<BTreeSet<_>>();
    let expected_authorization_ids = GROUP_AUTHORIZATION_CASE_IDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    ensure_exact(&group_authorization_ids, &expected_authorization_ids)
}

fn fixture_document(input: &str) -> Result<Value, RepositoryError> {
    serde_json::from_str(input).map_err(|_| RepositoryError::Unavailable)
}

fn member_ids<'a>(
    document: &'a Value,
    collection: &str,
) -> Result<BTreeSet<&'a str>, RepositoryError> {
    let members = document
        .get(collection)
        .and_then(Value::as_array)
        .ok_or(RepositoryError::Unavailable)?;
    let ids = members
        .iter()
        .map(|member| {
            member
                .get("id")
                .and_then(Value::as_str)
                .ok_or(RepositoryError::Unavailable)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if ids.len() == members.len() {
        Ok(ids)
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn ensure_cloud_ownership(document: &Value) -> Result<(), RepositoryError> {
    let cloud_ids = document
        .get("cases")
        .and_then(Value::as_array)
        .ok_or(RepositoryError::Unavailable)?
        .iter()
        .filter(|member| member.get("conformanceOwner").and_then(Value::as_str) == Some("cloud"))
        .filter_map(|member| member.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let expected = CLOUD_CASE_IDS.iter().copied().collect::<BTreeSet<_>>();
    ensure_exact(&cloud_ids, &expected)
}

fn ensure_exact<T: Ord>(
    actual: &BTreeSet<T>,
    expected: &BTreeSet<T>,
) -> Result<(), RepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use blobyard_contract::RepositoryError;
    use serde_json::{Value, json};

    #[test]
    fn generated_group_admission_fixtures_have_exact_execution_owners()
    -> Result<(), RepositoryError> {
        super::group_admission_fixture_conformance()
    }

    #[test]
    fn malformed_fixture_structures_fail_closed() {
        assert_unavailable(&super::fixture_document("{"));

        for document in [json!({}), json!({"cases": {}})] {
            assert_unavailable(&super::member_ids(&document, "cases"));
            assert_unavailable(&super::ensure_cloud_ownership(&document));
        }

        for document in [
            json!({"cases": [{}]}),
            json!({"cases": [{"id": 1}]}),
            json!({"cases": [{"id": "duplicate"}, {"id": "duplicate"}]}),
        ] {
            assert_unavailable(&super::member_ids(&document, "cases"));
        }

        for document in [
            json!({"cases": [{"conformanceOwner": "cloud"}]}),
            json!({
                "cases": [{
                    "conformanceOwner": 1,
                    "id": "same-user-in-two-workspaces-does-not-bridge-authority"
                }]
            }),
            json!({
                "cases": [{
                    "conformanceOwner": "cloud",
                    "id": "unexpected-cloud-owner"
                }]
            }),
        ] {
            assert_unavailable(&super::ensure_cloud_ownership(&document));
        }
    }

    #[test]
    fn full_fixture_validation_propagates_each_failure() {
        let yard_document =
            super::fixture_document(super::YARD_SESSION_FIXTURE).unwrap_or(Value::Null);
        let authorization_document =
            super::fixture_document(super::AUTHORIZATION_FIXTURE).unwrap_or(Value::Null);

        assert_fixture_failure("{", super::AUTHORIZATION_FIXTURE);
        assert_fixture_failure("{}", super::AUTHORIZATION_FIXTURE);

        let mut unknown_yard_case = yard_document.clone();
        unknown_yard_case["cases"][0]["id"] = json!("unknown-yard-case");
        assert_fixture_failure(&unknown_yard_case.to_string(), super::AUTHORIZATION_FIXTURE);

        let mut missing_cloud_owner = yard_document;
        missing_cloud_owner["cases"][35]["conformanceOwner"] = Value::Null;
        assert_fixture_failure(
            &missing_cloud_owner.to_string(),
            super::AUTHORIZATION_FIXTURE,
        );

        assert_fixture_failure(super::YARD_SESSION_FIXTURE, "{");
        assert_fixture_failure(super::YARD_SESSION_FIXTURE, "{}");

        let mut unknown_authorization_case = authorization_document;
        unknown_authorization_case["vectors"][2]["id"] = json!("unknown-authorization-case");
        assert_fixture_failure(
            super::YARD_SESSION_FIXTURE,
            &unknown_authorization_case.to_string(),
        );
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
}
