use super::super::{cron, patterns};

#[test]
fn validates_identifier_and_capability_patterns() {
    assert_cases(
        patterns::dns_label,
        &["a", "a-1"],
        &["", "-a", "a-", "A", &"a".repeat(64)],
    );
    assert_cases(
        patterns::role_name,
        &["a", "role-1"],
        &["", "1role", "Role", "role_name", &"a".repeat(33)],
    );
    assert_cases(
        patterns::permission,
        &["risks.read", "a.b_c.d1.e"],
        &[
            "risk",
            ".read",
            "risks.",
            "Risks.read",
            "risks-read",
            &format!("a.{}", "b".repeat(127)),
        ],
    );
    assert_cases(
        patterns::secret_name,
        &["A", "API_TOKEN_1"],
        &["", "1TOKEN", "token", "API-TOKEN", &"A".repeat(65)],
    );
    assert_cases(
        patterns::bucket_grant,
        &["files:read", "files:read-write"],
        &["files", "Files:read", "files:admin", "files:read:write"],
    );
}

#[test]
fn validates_network_and_path_patterns() {
    assert_cases(
        patterns::egress_target,
        &["api.example.com:443", "example.co.uk:80"],
        &[
            "example.com:81",
            "localhost:443",
            "-api.example.com:443",
            "api.example.c:443",
            "api.Example.com:443",
            "missing-port.example.com",
            &format!("{}.example.com:443", "a".repeat(241)),
        ],
    );
    assert_cases(
        patterns::relative_path,
        &["dist", "assets/app.js", ".../file"],
        &[
            "",
            "/dist",
            "../dist",
            "./dist",
            "dist/",
            "dist//app",
            "dist\\app",
            &"a".repeat(257),
        ],
    );
    assert_cases(
        patterns::module_path,
        &["function.ts", "functions/app.mjs"],
        &[
            ".ts",
            "function.rs",
            "../function.ts",
            "/function.js",
            &format!("{}.ts", "a".repeat(254)),
        ],
    );
    assert_cases(
        patterns::route_path,
        &["/", "/risks", "/risks/~mine_1.json"],
        &[
            "",
            "risks",
            "/risks/",
            "/risks//one",
            "/risks?query",
            &format!("/{}", "a".repeat(256)),
        ],
    );
}

#[test]
fn validates_size_schedule_and_timezone_patterns() {
    assert_cases(
        patterns::byte_size,
        &["1KiB", "999999GiB"],
        &["0KiB", "1000000MiB", "1KB", "KiB"],
    );
    assert_cases(
        patterns::duration,
        &["1ms", "9999m"],
        &["0s", "10000s", "1h", "ms"],
    );
    assert_cases(
        patterns::cron_shape,
        &["0 9 * * 1"],
        &[
            "0  9 * * 1",
            "0\t9 * * 1",
            "0 9 * *",
            &format!("{} 9 * * 1", "1".repeat(65)),
        ],
    );
    assert_cases(
        patterns::timezone_shape,
        &[
            "UTC",
            "Europe/London",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+1",
        ],
        &[
            "",
            "London",
            "Europe/",
            "1urope/London",
            "Europe/London/Extra/Part",
            "Europe/Lon.don",
            &format!("Europe/{}", "a".repeat(60)),
        ],
    );
}

#[test]
fn parses_standard_five_field_cron_expressions() {
    for valid in [
        "0 9 * * 1",
        "*/15 0-23/2 1,15 * MON-FRI",
        "5/10 8 1 JAN sun",
        "0 0 31 12 7",
    ] {
        assert!(cron::valid(valid), "valid cron: {valid}");
    }
    for invalid in [
        "0 9 * *",
        "0 9 * * 1 extra",
        "60 9 * * 1",
        "0 24 * * 1",
        "0 9 0 * 1",
        "0 9 * 13 1",
        "0 9 * * 8",
        "*/0 9 * * 1",
        "*/x 9 * * 1",
        "0 9 5-1 * 1",
        "0 9 1--5 * 1",
        "0 9 * FOO 1",
        "0 9 * * MON/2/3",
        "0 9 * * ",
    ] {
        assert!(!cron::valid(invalid), "invalid cron: {invalid}");
    }
}

fn assert_cases(predicate: fn(&str) -> bool, valid: &[&str], invalid: &[&str]) {
    for value in valid {
        assert!(predicate(value), "expected valid: {value}");
    }
    for value in invalid {
        assert!(!predicate(value), "expected invalid: {value}");
    }
}
