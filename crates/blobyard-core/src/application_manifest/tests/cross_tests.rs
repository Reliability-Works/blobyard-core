use super::super::ApplicationManifest;
use super::{COMPLETE, MINIMAL, errors};

#[test]
fn validates_role_references_permissions_and_acyclic_inheritance() {
    let source = format!(
        r#"{MINIMAL}
[auth]
default_role = "missing"

[auth.roles.reader]
inherits = ["ghost"]
permissions = ["items.read"]

[auth.roles.writer]
inherits = ["admin"]

[auth.roles.admin]
inherits = ["writer"]

[[functions]]
name = "create"
entry = "create.ts"
type = "rpc"
permissions = ["items.write"]
"#
    );
    let failures = errors(&source);
    assert_has(&failures, "auth.default_role", "undeclared role `missing`");
    assert_has(
        &failures,
        "auth.roles.reader.inherits[0]",
        "undeclared role `ghost`",
    );
    assert_has(&failures, "auth.roles.writer.inherits", "acyclic");
    assert_has(&failures, "auth.roles.admin.inherits", "acyclic");
    assert_has(
        &failures,
        "functions[0].permissions[0]",
        "not declared by any auth role",
    );

    let rendered = ApplicationManifest::parse_toml(&source)
        .expect_err("role failures")
        .to_string();
    assert!(rendered.lines().count() >= 5);
}

#[test]
fn validates_job_route_and_health_function_references() {
    let source = format!(
        r#"{MINIMAL}
[[functions]]
name = "rpc-task"
entry = "rpc.ts"
type = "rpc"

[[jobs]]
name = "wrong-type"
function = "rpc-task"
schedule = "0 0 * * 1"
timezone = "UTC"

[[jobs]]
name = "missing"
function = "missing"
schedule = "0 0 * * 1"
timezone = "UTC"

[[routes]]
path = "/missing"
method = "GET"
function = "missing"

[health]
function = "missing"
"#
    );
    let failures = errors(&source);
    assert_has(&failures, "jobs[0].function", "scheduled function");
    assert_has(
        &failures,
        "jobs[1].function",
        "undeclared function `missing`",
    );
    assert_has(
        &failures,
        "routes[0].function",
        "undeclared function `missing`",
    );
    assert_has(
        &failures,
        "health.function",
        "undeclared function `missing`",
    );
    assert_eq!(failures.len(), 4);
}

#[test]
fn validates_bucket_grants_and_database_section_requirement() {
    let source = format!(
        r#"{MINIMAL}
[[buckets]]
name = "declared"

[[functions]]
name = "consumer"
entry = "consumer.ts"
type = "rpc"
database = "read"
buckets = ["declared:read", "missing:write"]
"#
    );
    let failures = errors(&source);
    assert_has(
        &failures,
        "functions[0].buckets[1]",
        "undeclared bucket `missing`",
    );
    assert_has(
        &failures,
        "functions[0].database",
        "requires a database section",
    );
    assert_eq!(failures.len(), 2);

    let none = format!(
        "{MINIMAL}\n[[functions]]\nname = \"consumer\"\nentry = \"consumer.ts\"\ntype = \"rpc\"\ndatabase = \"none\"\n"
    );
    ApplicationManifest::parse_toml(&none).expect("none needs no database section");
}

#[test]
fn validates_event_and_queue_field_pairing_for_every_function_type() {
    let source = format!(
        r#"{MINIMAL}
[[functions]]
name = "event-missing"
entry = "event.ts"
type = "event"

[[functions]]
name = "event-queue"
entry = "event-queue.ts"
type = "event"
event = "items.created"
queue = "items"

[[functions]]
name = "queue-missing"
entry = "queue.ts"
type = "queue"

[[functions]]
name = "queue-event"
entry = "queue-event.ts"
type = "queue"
queue = "items"
event = "items.created"

[[functions]]
name = "plain"
entry = "plain.ts"
type = "http"
event = "items.created"
queue = "items"
"#
    );
    let failures = errors(&source);
    for path in [
        "functions[0].event",
        "functions[1].queue",
        "functions[2].queue",
        "functions[3].event",
        "functions[4].event",
        "functions[4].queue",
    ] {
        assert!(
            failures.iter().any(|failure| failure.0 == path),
            "missing {path}"
        );
    }
    assert_eq!(failures.len(), 6);
}

#[test]
fn validates_route_uniqueness_cron_and_canonical_iana_timezone() {
    let source = format!(
        r#"{MINIMAL}
[[functions]]
name = "scheduled"
entry = "scheduled.ts"
type = "scheduled"

[[functions]]
name = "http"
entry = "http.ts"
type = "http"

[[jobs]]
name = "invalid-cron"
function = "scheduled"
schedule = "60 0 * * 1"
timezone = "Europe/London"

[[jobs]]
name = "invalid-zone"
function = "scheduled"
schedule = "0 0 * * 1"
timezone = "Europe/Nowhere"

[[jobs]]
name = "wrong-case"
function = "scheduled"
schedule = "0 0 * * 1"
timezone = "europe/london"

[[routes]]
path = "/items"
method = "GET"
function = "http"

[[routes]]
path = "/items"
method = "GET"
function = "http"

[[routes]]
path = "/items"
method = "POST"
function = "http"
"#
    );
    let failures = errors(&source);
    assert_has(&failures, "jobs[0].schedule", "valid five-field cron");
    assert_has(&failures, "jobs[1].timezone", "canonical IANA timezone");
    assert_has(&failures, "jobs[2].timezone", "canonical IANA timezone");
    assert_has(&failures, "routes[1].path", "unique for its method");
    assert_eq!(failures.len(), 4);
}

#[test]
fn keeps_declared_capabilities_scoped_to_each_function() {
    let manifest = ApplicationManifest::parse_toml(COMPLETE).expect("complete manifest");
    let functions = manifest.functions.expect("functions");
    assert_eq!(
        functions[0].secrets.as_deref(),
        Some(["API_TOKEN".to_owned()].as_slice())
    );
    assert!(functions[1].secrets.is_none());
    assert!(functions[1].network.is_none());
    assert_eq!(functions[0].email, Some(true));
    assert_eq!(functions[1].email, None);
}

fn assert_has(failures: &[(String, String)], path: &str, message: &str) {
    assert!(
        failures
            .iter()
            .any(|failure| failure.0 == path && failure.1.contains(message)),
        "missing {path}: {message}; got {failures:?}"
    );
}
