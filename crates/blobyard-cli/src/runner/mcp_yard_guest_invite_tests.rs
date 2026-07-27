use super::mcp_yard_command;
use crate::Command;
use crate::yard_commands::GuestInvitesCommand;
use blobyard_mcp::{Scope, ToolCall, WebYardToolCall, YardGuestInviteToolCall};

#[test]
fn guest_invitation_tools_map_to_management_only_cli_commands() {
    let scope = Scope::default();
    let (_, listed) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::GuestInvite(
        YardGuestInviteToolCall::List {
            scope: scope.clone(),
            yard: "site".into(),
            cursor: Some("next".into()),
        },
    )))
    .expect("guest list mapping");
    assert!(matches!(
        listed,
        Command::GuestInvites {
            command: GuestInvitesCommand::List(arguments)
        } if arguments.name.as_deref() == Some("site")
            && arguments.cursor.as_deref() == Some("next")
    ));

    let (_, created) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::GuestInvite(
        YardGuestInviteToolCall::Create {
            scope: scope.clone(),
            yard: "site".into(),
            email: "guest@example.com".into(),
            roles: vec!["viewer".into()],
            environment_id: None,
            expires_at: Some("2026-08-03T09:00:00Z".into()),
        },
    )))
    .expect("guest create mapping");
    assert!(matches!(
        created,
        Command::GuestInvites {
            command: GuestInvitesCommand::Create(arguments)
        } if arguments.email == "guest@example.com"
            && arguments.roles == ["viewer"]
    ));

    let (_, revoked) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::GuestInvite(
        YardGuestInviteToolCall::Revoke {
            scope,
            yard: "site".into(),
            invitation_id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        },
    )))
    .expect("guest revoke mapping");
    assert!(matches!(
        revoked,
        Command::GuestInvites {
            command: GuestInvitesCommand::Revoke(arguments)
        } if arguments.invitation_id == "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ));
}
