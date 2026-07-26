use super::super::{ApplicationManifest, schema};
use super::{MINIMAL, errors};

#[test]
fn reports_toml_and_root_shape_failures() {
    let invalid = ApplicationManifest::parse_toml("schema_version = [").expect_err("invalid TOML");
    assert_eq!(invalid.errors()[0].path(), "$");
    assert!(invalid.errors()[0].message().starts_with("invalid TOML:"));
    assert!(invalid.to_string().contains("$: invalid TOML:"));

    let failures = schema::validate(&toml::Value::String("not an object".to_owned()));
    assert_eq!(failures[0].to_string(), "$: must be an object");
}

#[test]
fn rejects_unknown_missing_typed_const_and_structural_constraints() {
    let source = r#"
schema_version = "1"
application = "wrong"
frontend = []
auth = []
database = []
buckets = {}
functions = {}
jobs = {}
routes = {}
limits = []
health = []
unknown = true
"#;
    let failures = errors(source);
    assert_paths(
        &failures,
        &[
            "unknown",
            "schema_version",
            "application",
            "frontend",
            "auth",
            "database",
            "buckets",
            "functions",
            "jobs",
            "routes",
            "limits",
            "health",
        ],
    );

    let failures = errors("schema_version = 1");
    assert_paths(&failures, &["application"]);
    let failures = errors("application = {}\n");
    assert_paths(
        &failures,
        &["schema_version", "application.name", "application.runtime"],
    );

    for (suffix, path) in [
        ("[auth]\nroles = \"wrong\"\n", "auth.roles"),
        (
            "[auth]\nroles = { reader = \"wrong\" }\n",
            "auth.roles.reader",
        ),
        ("[limits]\nfunction_class = 1\n", "limits.function_class"),
        (
            "[[functions]]\nname = \"task\"\nentry = \"task.ts\"\ntype = \"rpc\"\npermissions = \"wrong\"\n",
            "functions[0].permissions",
        ),
    ] {
        assert_paths(&errors(&format!("{MINIMAL}\n{suffix}")), &[path]);
    }

    let source = format!(
        "buckets = [\"wrong\"]\nfunctions = [\"wrong\"]\njobs = [\"wrong\"]\nroutes = [\"wrong\"]\n{MINIMAL}"
    );
    assert_paths(
        &errors(&source),
        &["buckets[0]", "functions[0]", "jobs[0]", "routes[0]"],
    );
}

#[test]
fn validates_required_fields_when_optional_sections_are_empty() {
    let application_only =
        "schema_version = 1\n[application]\nname = \"example-app\"\nruntime = \"blobyard-js-1\"\n";
    assert_paths(
        &errors(&format!("{application_only}[frontend]\n")),
        &["frontend.directory"],
    );
    assert_paths(
        &errors(&format!(
            "{MINIMAL}\n[[functions]]\nname = \"task\"\nentry = \"task.ts\"\n"
        )),
        &["functions[0].type"],
    );

    ApplicationManifest::parse_toml(&format!("{MINIMAL}\n[auth]\n"))
        .expect("empty optional auth section");
    ApplicationManifest::parse_toml(&format!(
        "{MINIMAL}\n[[jobs]]\nname = \"job\"\nfunction = \"task\"\nschedule = \"0 0 * * 1\"\ntimezone = \"UTC\"\n[jobs.retry]\n"
    ))
    .expect_err("empty retry reaches cross-field validation");
}

const INVALID_NESTED: &str = r#"
schema_version = 2
extra = true

[application]
name = "Invalid"
runtime = "node"
extra = true

[frontend]
directory = "../dist"
spa_fallback = "yes"
clean_urls = 1
extra = true

[auth]
default_role = 1
extra = true

[auth.roles.Bad]
inherits = ["missing", "missing"]
permissions = ["invalid", "invalid"]
extra = true

[database]
extra = true

[[buckets]]
visibility = "shared"
max_object_size = "0MiB"
extra = true

[[buckets]]
name = 1

[[functions]]
name = "Bad"
entry = "../bad.rs"
type = "worker"
permissions = ["invalid", "invalid"]
database = "admin"
buckets = ["missing:admin", "missing:admin"]
secrets = ["bad", "bad"]
network = ["localhost:22", "localhost:22"]
email = "yes"
event = "bad"
queue = "Bad"
extra = true

[[functions]]
type = "rpc"

[[jobs]]
name = "Bad"
function = "Bad"
schedule = "not cron"
timezone = "London"
extra = true

[jobs.retry]
max_attempts = 11
backoff = "linear"
extra = true

[[jobs]]
name = "job"
function = "task"
schedule = "0 0 * * 1"
timezone = "UTC"
retry = "wrong"

[[routes]]
path = "relative"
method = "OPTIONS"
function = "Bad"
auth = "maybe"
extra = true

[[routes]]
auth = "required"

[limits]
function_class = "large"
function_timeout = "0s"
concurrency = 101
extra = true

[health]
timeout = "5h"
extra = true
"#;

const INVALID_NESTED_PATHS: &[&str] = &[
    "schema_version",
    "extra",
    "application.name",
    "application.runtime",
    "application.extra",
    "frontend.directory",
    "frontend.spa_fallback",
    "frontend.clean_urls",
    "frontend.extra",
    "auth.default_role",
    "auth.extra",
    "auth.roles.Bad",
    "auth.roles.Bad.inherits[1]",
    "auth.roles.Bad.permissions[0]",
    "auth.roles.Bad.permissions[1]",
    "auth.roles.Bad.extra",
    "database.migrations",
    "database.extra",
    "buckets[0].name",
    "buckets[0].visibility",
    "buckets[0].max_object_size",
    "buckets[0].extra",
    "buckets[1].name",
    "functions[0].name",
    "functions[0].entry",
    "functions[0].type",
    "functions[0].permissions[0]",
    "functions[0].permissions[1]",
    "functions[0].database",
    "functions[0].buckets[0]",
    "functions[0].buckets[1]",
    "functions[0].secrets[0]",
    "functions[0].network[0]",
    "functions[0].email",
    "functions[0].event",
    "functions[0].queue",
    "functions[0].extra",
    "functions[1].name",
    "functions[1].entry",
    "jobs[0].name",
    "jobs[0].function",
    "jobs[0].schedule",
    "jobs[0].timezone",
    "jobs[0].extra",
    "jobs[0].retry.max_attempts",
    "jobs[0].retry.backoff",
    "jobs[0].retry.extra",
    "jobs[1].retry",
    "routes[0].path",
    "routes[0].method",
    "routes[0].function",
    "routes[0].auth",
    "routes[0].extra",
    "routes[1].path",
    "routes[1].method",
    "routes[1].function",
    "limits.function_class",
    "limits.function_timeout",
    "limits.concurrency",
    "limits.extra",
    "health.function",
    "health.timeout",
    "health.extra",
];

#[test]
fn rejects_nested_schema_constraints_with_precise_paths() {
    let failures = errors(INVALID_NESTED);
    for path in INVALID_NESTED_PATHS {
        assert!(
            failures.iter().any(|failure| failure.0 == *path),
            "missing failure: {path}"
        );
    }
    assert!(failures.len() > 60);
}

fn assert_paths(failures: &[(String, String)], expected: &[&str]) {
    for path in expected {
        assert!(
            failures.iter().any(|failure| failure.0 == *path),
            "missing failure: {path}"
        );
    }
}
