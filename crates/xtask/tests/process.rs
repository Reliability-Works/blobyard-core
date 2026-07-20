//! Instrumented xtask binary behavior tests.

#![allow(clippy::expect_used, reason = "spawning the test binary must succeed")]

use std::process::Command;

const XTASK_BIN: &str = env!("CARGO_BIN_EXE_xtask");

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(XTASK_BIN)
        .args(arguments)
        .output()
        .expect("instrumented xtask binary must run")
}

#[test]
fn binary_runs_the_repository_limit_check() {
    let output = run(&["check-limits"]);

    assert!(output.status.success());
    assert_eq!(output.stdout, b"Rust source limits passed.\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn binary_rejects_missing_or_extra_commands() {
    for arguments in [&[][..], &["check-limits", "extra"][..]] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            b"usage: cargo run -p xtask -- check-limits\n"
        );
    }
}
