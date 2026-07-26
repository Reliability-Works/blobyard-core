#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::headless_commands::{CursorArgs, GroupNameArgs};
use crate::runner::login::tests::support::Fixture;

#[tokio::test]
async fn group_execution_propagates_validation_and_authentication_failures() {
    let fixture = Fixture::new(&["blobyard", "--workspace", "main", "whoami"], vec![]);
    assert_eq!(
        fixture
            .runner
            .execute_groups_command(&GroupsCommand::Create(GroupNameArgs {
                name: "x".to_owned(),
            }))
            .await
            .expect_err("invalid name")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        fixture
            .runner
            .execute_groups_command(&GroupsCommand::List(CursorArgs { cursor: None }))
            .await
            .expect_err("missing authentication")
            .code(),
        ErrorCode::AuthRequired
    );
}
