#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::validate_user_arguments;
use crate::headless_commands::CreateUserArgs;
use blobyard_core::ErrorCode;

fn arguments(display_name: &str, email: Option<&str>) -> CreateUserArgs {
    CreateUserArgs {
        display_name: display_name.to_owned(),
        email: email.map(str::to_owned),
    }
}

#[test]
fn user_arguments_accept_valid_names_and_emails() {
    assert!(validate_user_arguments(&arguments("Ada Lovelace", None)).is_ok());
    assert!(validate_user_arguments(&arguments("Ada", Some("ada@example.test"))).is_ok());
}

#[test]
fn user_arguments_reject_invalid_names_and_emails() {
    let long_name = "x".repeat(81);
    let long_email = format!("a@{}", "b".repeat(254));
    let cases = [
        arguments(" ", None),
        arguments(&long_name, None),
        arguments("line\nbreak", None),
        arguments("Ada", Some("missing-at")),
        arguments("Ada", Some("split @example.test")),
        arguments("Ada", Some(&long_email)),
    ];
    for case in cases {
        assert_eq!(
            validate_user_arguments(&case)
                .expect_err("invalid arguments")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}
