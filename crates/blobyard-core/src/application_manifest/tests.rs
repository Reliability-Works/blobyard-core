#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

mod cross_tests;
mod pattern_tests;
mod schema_bounds_tests;
mod schema_tests;

use super::{ApplicationManifest, ApplicationRuntime, DatabaseAccess, FunctionType};
use std::fmt::{self, Write};

struct FailingWriter(usize);

impl Write for FailingWriter {
    fn write_str(&mut self, _value: &str) -> fmt::Result {
        if self.0 == 0 {
            return Err(fmt::Error);
        }
        self.0 -= 1;
        Ok(())
    }
}

pub(super) const MINIMAL: &str = r#"
schema_version = 1

[application]
name = "example-app"
runtime = "blobyard-js-1"

[frontend]
directory = "dist"
"#;

pub(super) const COMPLETE: &str = r#"
schema_version = 1

[application]
name = "risk-tracker"
runtime = "blobyard-js-1"

[frontend]
directory = "dist"
spa_fallback = true
clean_urls = false

[auth]
default_role = "viewer"

[auth.roles.viewer]
permissions = ["risks.read"]

[auth.roles.editor]
inherits = ["viewer"]
permissions = ["risks.write"]

[database]
migrations = "migrations"

[[buckets]]
name = "attachments"
visibility = "public-read"
max_object_size = "50MiB"

[[functions]]
name = "risk-create"
entry = "functions/risk-create.ts"
type = "rpc"
permissions = ["risks.write"]
database = "read-write"
buckets = ["attachments:read-write"]
secrets = ["API_TOKEN"]
network = ["api.example.com:443"]
email = true

[[functions]]
name = "weekly-summary"
entry = "functions/weekly-summary.mjs"
type = "scheduled"
database = "read"

[[functions]]
name = "events"
entry = "functions/events.js"
type = "event"
event = "risks.created"

[[functions]]
name = "queue"
entry = "functions/queue.mts"
type = "queue"
queue = "risk-jobs"

[[jobs]]
name = "weekly-summary"
function = "weekly-summary"
schedule = "0 9 * * MON"
timezone = "Europe/London"

[jobs.retry]
max_attempts = 5
backoff = "fixed"

[[routes]]
path = "/risks"
method = "POST"
function = "risk-create"
auth = "public"

[limits]
function_class = "standard"
function_timeout = "30s"
concurrency = 10

[health]
function = "risk-create"
timeout = "5s"
"#;

#[test]
fn parses_minimal_manifest() {
    let minimal = ApplicationManifest::parse_toml(MINIMAL).expect("minimal manifest");
    assert_eq!(minimal.application.name, "example-app");
    assert_eq!(minimal.application.runtime, ApplicationRuntime::BlobyardJs1);
    assert_eq!(minimal.application.runtime.as_str(), "blobyard-js-1");
    assert_eq!(minimal.capability_counts().functions, 0);
}

#[test]
fn parses_complete_manifest_and_counts_capabilities() {
    let complete = ApplicationManifest::parse_toml(COMPLETE).expect("complete manifest");
    assert_eq!(
        complete.functions.as_ref().expect("functions")[0].function_type,
        FunctionType::Rpc
    );
    assert_eq!(
        complete.functions.as_ref().expect("functions")[0].database,
        Some(DatabaseAccess::ReadWrite)
    );
    assert_eq!(complete.capability_counts().roles, 2);
    assert_eq!(complete.capability_counts().databases, 1);
    assert_eq!(complete.capability_counts().buckets, 1);
    assert_eq!(complete.capability_counts().functions, 4);
    assert_eq!(complete.capability_counts().jobs, 1);
    assert_eq!(complete.capability_counts().routes, 1);
    assert_eq!(complete.capability_counts().secrets, 1);
    assert_eq!(complete.capability_counts().network_targets, 1);
    assert_eq!(complete.capability_counts().email_functions, 1);
}

#[test]
fn canonical_json_is_stable_and_preserves_direct_projection() {
    let manifest = ApplicationManifest::parse_toml(MINIMAL).expect("manifest");
    let canonical = manifest.canonical_json().expect("canonical JSON");
    assert_eq!(
        canonical,
        "{\"schema_version\":1,\"application\":{\"name\":\"example-app\",\"runtime\":\"blobyard-js-1\"},\"frontend\":{\"directory\":\"dist\"}}"
    );
    assert_eq!(canonical, manifest.canonical_json().expect("repeat"));
}

#[test]
fn internal_decode_and_empty_error_rendering_fail_safely() {
    let invalid = ApplicationManifest::parse_validated(toml::Value::String("wrong".to_owned()))
        .expect_err("invalid typed projection");
    assert!(
        invalid
            .to_string()
            .contains("manifest could not be decoded")
    );
    assert_eq!(super::ManifestErrors::new(Vec::new()).to_string(), "");
    let failures = super::ManifestErrors::new(vec![
        super::ManifestError::new("first", "failure"),
        super::ManifestError::new("second", "failure"),
    ]);
    assert_eq!(failures.to_string(), "first: failure\nsecond: failure");

    let formatter_failures = (0..8)
        .filter(|limit| write!(FailingWriter(*limit), "{failures}").is_err())
        .count();
    assert!(formatter_failures > 1);
}

pub(super) fn errors(source: &str) -> Vec<(String, String)> {
    ApplicationManifest::parse_toml(source)
        .expect_err("manifest must fail")
        .errors()
        .iter()
        .map(|error| (error.path().to_owned(), error.message().to_owned()))
        .collect()
}
