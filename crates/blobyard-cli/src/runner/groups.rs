use super::{Runner, command_result};
use crate::headless_commands::GroupsCommand;
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{AdminToolCall, GroupToolCall, Scope};

impl Runner {
    pub(super) async fn execute_groups_command(
        &self,
        command: &GroupsCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let call = group_call(command)?;
        let success = self
            .execute_mcp_admin_success(AdminToolCall::Group(call))
            .await?;
        let human = group_human(command, success.data());
        command_result(success.data(), human, success.request_id())
    }
}

fn group_call(command: &GroupsCommand) -> Result<GroupToolCall, BlobyardError> {
    let scope = Scope::default();
    let call = match command {
        GroupsCommand::List(arguments) => GroupToolCall::List {
            scope,
            cursor: arguments.cursor.clone(),
        },
        GroupsCommand::Create(arguments) => GroupToolCall::Create {
            scope,
            name: arguments.name.clone(),
        },
        GroupsCommand::Rename(arguments) => GroupToolCall::Rename {
            scope,
            group_id: arguments.group_id.clone(),
            name: arguments.name.clone(),
        },
        GroupsCommand::Members(arguments) => GroupToolCall::ListMembers {
            scope,
            group_id: arguments.group_id.clone(),
            cursor: arguments.cursor.clone(),
        },
        GroupsCommand::AddMember(arguments) => GroupToolCall::AddMember {
            scope,
            group_id: arguments.group_id.clone(),
            user_id: arguments.user_id.clone(),
        },
        GroupsCommand::RemoveMember(arguments) => GroupToolCall::RemoveMember {
            scope,
            group_id: arguments.group_id.clone(),
            user_id: arguments.user_id.clone(),
            confirmed: true,
        },
        GroupsCommand::Deactivate(arguments) => GroupToolCall::Deactivate {
            scope,
            group_id: arguments.group_id.clone(),
            confirmed: true,
        },
    };
    validate(&call)?;
    Ok(call)
}

fn validate(call: &GroupToolCall) -> Result<(), BlobyardError> {
    let valid = match call {
        GroupToolCall::Create { name, .. } | GroupToolCall::Rename { name, .. } => {
            let count = name.trim_matches(char::is_whitespace).chars().count();
            (2..=80).contains(&count) && !name.chars().any(char::is_control)
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Group names must contain 2-80 printable characters.",
        ))
    }
}

fn group_human(command: &GroupsCommand, value: &serde_json::Value) -> String {
    match command {
        GroupsCommand::Create(_) => "Group created.".to_owned(),
        GroupsCommand::Rename(_) => "Group renamed.".to_owned(),
        GroupsCommand::AddMember(_) => "Group member added.".to_owned(),
        GroupsCommand::RemoveMember(_) => "Group member removed.".to_owned(),
        GroupsCommand::Deactivate(_) => "Group deactivated.".to_owned(),
        GroupsCommand::List(_) | GroupsCommand::Members(_) => format!("{value:#}"),
    }
}

#[cfg(test)]
#[path = "groups_edge_tests.rs"]
mod edge_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::*;
    use crate::TokenStore;
    use crate::headless_commands::{
        CursorArgs, GroupCursorArgs, GroupIdArgs, GroupMemberArgs, GroupNameArgs, RenameGroupArgs,
    };
    use crate::runner::login::tests::support::{Fixture, ok};
    use blobyard_api_client::Endpoint;
    use blobyard_core::SecretString;
    use serde_json::json;

    const GROUP_ID: &str = "group_0123456789abcdef0123456789abcdef";

    #[test]
    fn maps_every_group_command_to_a_validated_tool_call() {
        assert_list_and_create_calls();
        assert_rename_and_member_list_calls();
        assert_membership_calls();
    }

    fn assert_list_and_create_calls() {
        assert!(matches!(
            group_call(&GroupsCommand::List(CursorArgs {
                cursor: Some("next".to_owned())
            })),
            Ok(GroupToolCall::List { cursor: Some(value), .. }) if value == "next"
        ));
        assert!(matches!(
            group_call(&GroupsCommand::Create(GroupNameArgs {
                name: "Reviewers".to_owned()
            })),
            Ok(GroupToolCall::Create { name, .. }) if name == "Reviewers"
        ));
    }

    fn assert_rename_and_member_list_calls() {
        assert!(matches!(
            group_call(&GroupsCommand::Rename(RenameGroupArgs {
                group_id: GROUP_ID.to_owned(),
                name: "Approvers".to_owned()
            })),
            Ok(GroupToolCall::Rename { group_id, name, .. })
                if group_id == GROUP_ID && name == "Approvers"
        ));
        assert!(matches!(
            group_call(&GroupsCommand::Members(GroupCursorArgs {
                group_id: GROUP_ID.to_owned(),
                cursor: None
            })),
            Ok(GroupToolCall::ListMembers { group_id, cursor: None, .. }) if group_id == GROUP_ID
        ));
    }

    fn assert_membership_calls() {
        let arguments = GroupMemberArgs {
            group_id: GROUP_ID.to_owned(),
            user_id: "user_1".to_owned(),
        };
        assert!(matches!(
            group_call(&GroupsCommand::AddMember(arguments.clone())),
            Ok(GroupToolCall::AddMember { group_id, user_id, .. })
                if group_id == GROUP_ID && user_id == "user_1"
        ));
        assert!(matches!(
            group_call(&GroupsCommand::RemoveMember(arguments)),
            Ok(GroupToolCall::RemoveMember {
                confirmed: true,
                ..
            })
        ));
        assert!(matches!(
            group_call(&GroupsCommand::Deactivate(GroupIdArgs {
                group_id: GROUP_ID.to_owned()
            })),
            Ok(GroupToolCall::Deactivate {
                confirmed: true,
                ..
            })
        ));
    }

    #[test]
    fn rejects_invalid_group_names_before_transport() {
        for name in ["x", "x\n", &"x".repeat(81)] {
            let command = GroupsCommand::Create(GroupNameArgs {
                name: name.to_owned(),
            });
            assert_eq!(
                group_call(&command).map_err(|error| error.code()),
                Err(ErrorCode::InvalidRequest)
            );
        }
    }

    #[test]
    fn produces_stable_human_output_for_mutations_and_lists() {
        let value = json!({ "items": [] });
        assert_eq!(
            group_human(
                &GroupsCommand::Create(GroupNameArgs {
                    name: "Reviewers".to_owned()
                }),
                &value
            ),
            "Group created."
        );
        assert_eq!(
            group_human(
                &GroupsCommand::Rename(RenameGroupArgs {
                    group_id: GROUP_ID.to_owned(),
                    name: "Approvers".to_owned()
                }),
                &value
            ),
            "Group renamed."
        );
        assert_mutation_output(&value);
        assert!(
            group_human(&GroupsCommand::List(CursorArgs { cursor: None }), &value)
                .contains("\"items\"")
        );
        assert!(
            group_human(
                &GroupsCommand::Members(GroupCursorArgs {
                    group_id: GROUP_ID.to_owned(),
                    cursor: None
                }),
                &value
            )
            .contains("\"items\"")
        );
    }

    fn assert_mutation_output(value: &serde_json::Value) {
        let arguments = GroupMemberArgs {
            group_id: GROUP_ID.to_owned(),
            user_id: "user_1".to_owned(),
        };
        assert_eq!(
            group_human(&GroupsCommand::AddMember(arguments.clone()), value),
            "Group member added."
        );
        assert_eq!(
            group_human(&GroupsCommand::RemoveMember(arguments), value),
            "Group member removed."
        );
        assert_eq!(
            group_human(
                &GroupsCommand::Deactivate(GroupIdArgs {
                    group_id: GROUP_ID.to_owned()
                }),
                value
            ),
            "Group deactivated."
        );
    }

    #[tokio::test]
    async fn executes_group_commands_through_headless_dispatch() {
        let fixture = Fixture::new(
            &[
                "blobyard",
                "--api-url",
                "http://127.0.0.1:8787",
                "--workspace",
                "main",
                "groups",
                "list",
            ],
            vec![
                ok(
                    &json!({
                        "accessToken": "access-token-fixture",
                        "refreshToken": "refresh-token-fixture",
                        "expiresInSeconds": 900
                    }),
                    "req_refresh",
                ),
                ok(&json!({ "items": [], "nextCursor": null }), "req_groups"),
            ],
        );
        fixture
            .store
            .save(&SecretString::new("local-api-token").expect("token"))
            .expect("store token");

        let result = fixture
            .runner
            .execute_headless(&fixture.command)
            .await
            .expect("group list");
        assert_eq!(
            result.into_data(),
            json!({ "items": [], "nextCursor": null })
        );
        let requests = fixture.transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].endpoint(), Endpoint::ListGroups);
        assert_eq!(requests[1].query(), Some("workspace=main"));
    }
}
