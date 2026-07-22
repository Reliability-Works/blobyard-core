use super::{MINIMAL, errors};
use std::fmt::Write as _;

#[test]
fn enforces_collection_property_and_integer_bounds() {
    let mut roles = String::from("[auth]\n");
    for index in 0..33 {
        writeln!(roles, "[auth.roles.r{index}]").expect("string write");
    }
    assert_paths(&errors(&format!("{MINIMAL}\n{roles}")), &["auth.roles"]);
    assert_paths(
        &errors(&format!("{MINIMAL}\n[auth.roles]\n")),
        &["auth.roles"],
    );

    assert_max_items("buckets", 17, "name = \"item\"", "buckets");
    assert_max_items(
        "functions",
        65,
        "name = \"item\"\nentry = \"item.ts\"\ntype = \"rpc\"",
        "functions",
    );
    assert_max_items(
        "jobs",
        33,
        "name = \"item\"\nfunction = \"item\"\nschedule = \"0 0 * * 1\"\ntimezone = \"UTC\"",
        "jobs",
    );
    assert_max_items(
        "routes",
        65,
        "path = \"/item\"\nmethod = \"GET\"\nfunction = \"item\"",
        "routes",
    );

    let source = format!(
        "{MINIMAL}\n[[functions]]\nname = \"task\"\nentry = \"task.ts\"\ntype = \"rpc\"\npermissions = [{}]\n",
        vec!["\"scope.read\""; 17].join(", ")
    );
    assert_paths(
        &errors(&source),
        &["functions[0].permissions", "functions[0].permissions[1]"],
    );

    for value in ["0", "\"three\""] {
        let source = format!(
            "{MINIMAL}\n[[jobs]]\nname = \"job\"\nfunction = \"task\"\nschedule = \"0 0 * * 1\"\ntimezone = \"UTC\"\n[jobs.retry]\nmax_attempts = {value}\n"
        );
        assert_paths(&errors(&source), &["jobs[0].retry.max_attempts"]);
    }
    assert_paths(
        &errors(&format!("{MINIMAL}\n[limits]\nconcurrency = \"many\"\n")),
        &["limits.concurrency"],
    );
}

fn assert_max_items(section: &str, count: usize, body: &str, expected: &str) {
    let entries = format!("[[{section}]]\n{body}\n").repeat(count);
    assert_paths(&errors(&format!("{MINIMAL}\n{entries}")), &[expected]);
}

fn assert_paths(failures: &[(String, String)], expected: &[&str]) {
    for path in expected {
        assert!(
            failures.iter().any(|failure| failure.0 == *path),
            "missing failure: {path}"
        );
    }
}
