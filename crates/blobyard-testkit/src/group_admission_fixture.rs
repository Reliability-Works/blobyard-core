use blobyard_contract::RepositoryError;
use serde_json::Value;
use std::collections::BTreeSet;

const YARD_SESSION_FIXTURE: &str = include_str!("../../../conformance/behavior/yard-sessions.json");
const AUTHORIZATION_FIXTURE: &str = include_str!("../../../conformance/authorization/vectors.json");
const YARD_SESSION_CASE_COUNT: usize = 42;
const CLOUD_CASE_IDS: &[&str] = &[
    "same-user-in-two-workspaces-does-not-bridge-authority",
    "workspace-membership-removal-denies-on-next-private-request",
];
const OWNER_SUITES: &[(&str, &str)] = &[
    ("cloud", "hosted-admission"),
    ("server", "group-machine"),
    ("server", "session-contracts"),
    ("server", "session-cookie"),
    ("sqlite", "admission-corruption"),
    ("sqlite", "admission-drift"),
    ("sqlite", "group-limits"),
    ("sqlite", "group-pagination"),
    ("testkit", "groups"),
    ("testkit", "yard-sessions"),
];

/// Records the generated fixture cases reached by one executable conformance suite.
pub struct FixtureExecutionTracker {
    owner: &'static str,
    suite: &'static str,
    recorded: BTreeSet<String>,
    record_failed: bool,
}

impl FixtureExecutionTracker {
    /// Starts tracking an owner-local aggregate suite.
    #[must_use]
    pub const fn new(owner: &'static str, suite: &'static str) -> Self {
        Self {
            owner,
            suite,
            recorded: BTreeSet::new(),
            record_failed: false,
        }
    }

    /// Records one assertion reached by the running suite.
    ///
    /// Validation and duplicate failures accumulate until [`Self::finish`], so
    /// executable scenarios do not panic or return before completing their
    /// repository behavior.
    pub fn record_case(&mut self, id: &str, input: &Value, expected: &Value) {
        let valid = assert_fixture_member(
            YARD_SESSION_FIXTURE,
            "cases",
            id,
            Some(self.owner),
            Some(self.suite),
            input,
            expected,
        )
        .is_ok();
        if !valid || !self.recorded.insert(id.to_owned()) {
            self.record_failed = true;
        }
    }

    /// Requires the actual assertion calls to cover the suite's generated set exactly.
    ///
    /// # Errors
    ///
    /// Returns unavailable when the suite is unknown or any generated case was
    /// not reached by a valid, unique assertion call.
    pub fn finish(self) -> Result<(), RepositoryError> {
        if self.record_failed {
            return Err(RepositoryError::Unavailable);
        }
        fixture_execution_conformance_for(
            YARD_SESSION_FIXTURE,
            self.owner,
            self.suite,
            &self.recorded,
        )
    }
}

fn fixture_execution_conformance_for(
    fixture: &str,
    owner: &str,
    suite: &str,
    recorded: &BTreeSet<String>,
) -> Result<(), RepositoryError> {
    let document = fixture_document(fixture)?;
    let expected = members(&document, "cases")?
        .iter()
        .filter(|member| {
            member.get("conformanceOwner").and_then(Value::as_str) == Some(owner)
                && member.get("conformanceSuite").and_then(Value::as_str) == Some(suite)
        })
        .map(|member| {
            member
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(RepositoryError::Unavailable)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !expected.is_empty() && recorded == &expected {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

/// Verifies the structural invariants of the generated group/admission fixtures.
///
/// This inventory deliberately does not claim that a case executes. Executable
/// admission scenarios call [`FixtureExecutionTracker::record_case`] at each
/// assertion site and propagate [`FixtureExecutionTracker::finish`].
/// Authorization scenarios call [`assert_group_authorization_fixture_case`].
///
/// # Errors
///
/// Returns unavailable if generated JSON cannot be parsed, contains duplicate
/// identifiers, has an invalid execution owner, or drifts from the Cloud/Core
/// ownership boundary.
pub fn group_admission_fixture_conformance() -> Result<(), RepositoryError> {
    group_admission_fixture_conformance_for(YARD_SESSION_FIXTURE, AUTHORIZATION_FIXTURE)
}

/// Binds one executable scenario to its exact generated admission fixture.
///
/// # Errors
///
/// Returns unavailable when the identifier is absent or duplicated, the
/// execution owner differs, or either the input or expected value drifts.
pub fn assert_group_admission_fixture_case(
    id: &str,
    owner: &str,
    input: &Value,
    expected: &Value,
) -> Result<(), RepositoryError> {
    assert_fixture_member(
        YARD_SESSION_FIXTURE,
        "cases",
        id,
        Some(owner),
        None,
        input,
        expected,
    )
}

/// Binds one executable authorization scenario to its exact generated vector.
///
/// # Errors
///
/// Returns unavailable when the identifier is absent or duplicated, or when
/// any asserted vector field drifts.
pub fn assert_group_authorization_fixture_case(
    id: &str,
    asserted: &Value,
) -> Result<(), RepositoryError> {
    assert_exact_fixture_member(AUTHORIZATION_FIXTURE, "vectors", id, asserted)
}

fn assert_exact_fixture_member(
    fixture: &str,
    collection: &str,
    id: &str,
    asserted: &Value,
) -> Result<(), RepositoryError> {
    let document = fixture_document(fixture)?;
    let member = unique_member(&document, collection, id)?;
    if member == asserted {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn group_admission_fixture_conformance_for(
    yard_session_fixture: &str,
    authorization_fixture: &str,
) -> Result<(), RepositoryError> {
    let document = fixture_document(yard_session_fixture)?;
    let cases = members(&document, "cases")?;
    if cases.len() != YARD_SESSION_CASE_COUNT {
        return Err(RepositoryError::Unavailable);
    }
    unique_ids(cases)?;
    ensure_owners(cases)?;

    let authorization = fixture_document(authorization_fixture)?;
    unique_ids(members(&authorization, "vectors")?)?;
    Ok(())
}

fn assert_fixture_member(
    fixture: &str,
    collection: &str,
    id: &str,
    owner: Option<&str>,
    suite: Option<&str>,
    input: &Value,
    expected: &Value,
) -> Result<(), RepositoryError> {
    let document = fixture_document(fixture)?;
    let member = unique_member(&document, collection, id)?;
    let owner_matches = owner
        .is_none_or(|value| member.get("conformanceOwner").and_then(Value::as_str) == Some(value));
    let suite_matches = suite
        .is_none_or(|value| member.get("conformanceSuite").and_then(Value::as_str) == Some(value));
    if owner_matches
        && suite_matches
        && member.get("input") == Some(input)
        && member.get("expected") == Some(expected)
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn fixture_document(input: &str) -> Result<Value, RepositoryError> {
    serde_json::from_str(input).map_err(|_| RepositoryError::Unavailable)
}

fn members<'a>(document: &'a Value, collection: &str) -> Result<&'a [Value], RepositoryError> {
    document
        .get(collection)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(RepositoryError::Unavailable)
}

fn unique_ids(members: &[Value]) -> Result<BTreeSet<&str>, RepositoryError> {
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

fn unique_member<'a>(
    document: &'a Value,
    collection: &str,
    id: &str,
) -> Result<&'a Value, RepositoryError> {
    let mut matching = members(document, collection)?
        .iter()
        .filter(|member| member.get("id").and_then(Value::as_str) == Some(id));
    let member = matching.next().ok_or(RepositoryError::Unavailable)?;
    if matching.next().is_none() {
        Ok(member)
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn ensure_owners(cases: &[Value]) -> Result<(), RepositoryError> {
    let mut cloud_ids = BTreeSet::new();
    for member in cases {
        let owner = member
            .get("conformanceOwner")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::Unavailable)?;
        let suite = member
            .get("conformanceSuite")
            .and_then(Value::as_str)
            .ok_or(RepositoryError::Unavailable)?;
        if !OWNER_SUITES.contains(&(owner, suite)) {
            return Err(RepositoryError::Unavailable);
        }
        if owner == "cloud" {
            cloud_ids.insert(
                member
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(RepositoryError::Unavailable)?,
            );
        }
    }
    let expected = CLOUD_CASE_IDS.iter().copied().collect::<BTreeSet<_>>();
    if cloud_ids == expected {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

#[cfg(test)]
#[path = "group_admission_fixture_tests.rs"]
mod tests;
