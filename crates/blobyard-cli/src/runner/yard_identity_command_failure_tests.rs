#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::TokenStore;
use crate::runner::login::tests::support::{Fixture, api_failure, fixture_tokens, fixture_yards};
use blobyard_api_client::RawResponse;
use blobyard_core::{ErrorCode, SecretString};

async fn execute_identity_failure(
    arguments: &[&str],
    responses: Vec<RawResponse>,
    expected: ErrorCode,
) {
    let fixture = Fixture::new(arguments, responses);
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("identity command failure")
            .code(),
        expected
    );
}

#[tokio::test]
async fn identity_commands_propagate_selection_and_api_failures() {
    let command_tails = [
        vec!["management-roles", "list", "documentation"],
        vec![
            "management-roles",
            "set",
            "documentation",
            "user_developer",
            "developer",
        ],
        vec![
            "management-roles",
            "revoke",
            "documentation",
            "user_developer",
        ],
        vec!["application-policy", "get", "documentation"],
        vec![
            "application-policy",
            "set",
            "documentation",
            "--policy-json",
            r#"{"defaultRole":null,"roles":{}}"#,
            "--source-manifest-digest",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    ];
    for tail in command_tails {
        let mut missing = vec!["blobyard", "--workspace", "main", "--project", "artifacts"];
        missing.extend(tail.iter().copied());
        let yard_index = missing
            .iter()
            .position(|argument| *argument == "documentation")
            .expect("yard argument");
        missing[yard_index] = "missing-yard";
        execute_identity_failure(
            &missing,
            vec![fixture_tokens(), fixture_yards()],
            ErrorCode::NotFound,
        )
        .await;

        let mut api_failure_command =
            vec!["blobyard", "--workspace", "main", "--project", "artifacts"];
        api_failure_command.extend(tail);
        execute_identity_failure(
            &api_failure_command,
            vec![
                fixture_tokens(),
                fixture_yards(),
                fixture_tokens(),
                api_failure(ErrorCode::Conflict, 409, "request_identity_failed"),
            ],
            ErrorCode::Conflict,
        )
        .await;
    }
}

#[tokio::test]
async fn identity_mutations_propagate_local_input_failures() {
    execute_identity_failure(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "management-roles",
            "set",
            "documentation",
            "user_developer",
            "reader",
        ],
        Vec::new(),
        ErrorCode::InvalidRequest,
    )
    .await;
    execute_identity_failure(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "application-policy",
            "set",
            "documentation",
            "--policy-json",
            "{",
            "--source-manifest-digest",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
        Vec::new(),
        ErrorCode::InvalidRequest,
    )
    .await;
}
