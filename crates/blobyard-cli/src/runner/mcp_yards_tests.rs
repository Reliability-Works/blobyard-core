#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{mcp_yard_command, yard_policy_command};
use crate::Command;
use crate::yard_commands::{
    AccessCommand, ApplicationPolicyCommand, EnvCommand, ManagementRolesCommand, YardCommand,
    YardSessionsCommand,
};
use blobyard_core::ErrorCode;
use blobyard_mcp::{Scope, ToolCall, WebYardToolCall};
use serde_json::json;

#[test]
fn web_yard_tools_map_to_confirmed_cli_contracts() {
    let scope = Scope::default();
    let (_, deploy) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::DeployWebYard {
        scope: scope.clone(),
        directory: "./dist".into(),
        yard: "site".into(),
        spa: true,
        clean_urls: false,
    }))
    .expect("deploy mapping");
    assert!(matches!(deploy, Command::Deploy(arguments) if arguments.public));
    let (_, delete) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::DeleteWebYard {
        scope,
        yard: "site".into(),
    }))
    .expect("delete mapping");
    assert!(matches!(
        delete,
        Command::Yard {
            command: YardCommand::Delete(arguments)
        } if arguments.force
    ));
}

#[test]
fn web_yard_management_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, list) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListWebYards {
        scope: scope.clone(),
    }))
    .expect("list mapping");
    assert!(matches!(
        list,
        Command::Yard {
            command: YardCommand::List
        }
    ));
    let (_, history) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListYardDeploys {
        scope: scope.clone(),
        yard: "site".into(),
    }))
    .expect("history mapping");
    assert!(matches!(
        history,
        Command::Yard {
            command: YardCommand::History(_)
        }
    ));
    let (_, environments) =
        mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListYardEnvironments {
            scope: scope.clone(),
            yard: "site".into(),
        }))
        .expect("environment mapping");
    assert!(matches!(
        environments,
        Command::Env {
            command: EnvCommand::List(_)
        }
    ));
    let (_, rollback) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::RollbackWebYard {
        scope,
        yard: "site".into(),
        deploy_id: Some("deploy_1".into()),
    }))
    .expect("rollback mapping");
    assert!(matches!(
        rollback,
        Command::Yard {
            command: YardCommand::Rollback(_)
        }
    ));
}

#[test]
fn web_yard_access_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, list) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::GetYardAccess {
        scope: scope.clone(),
        yard: "site".into(),
    }))
    .expect("access list mapping");
    assert!(matches!(
        list,
        Command::Access {
            command: AccessCommand::List(arguments)
        } if arguments.name.as_deref() == Some("site")
    ));
    let (_, visibility) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::SetYardVisibility {
        scope,
        yard: "site".into(),
        visibility: "owner".into(),
    }))
    .expect("visibility mapping");
    assert!(matches!(
        visibility,
        Command::Access {
            command: AccessCommand::SetVisibility(arguments)
        } if arguments.visibility == "owner"
    ));
}

#[test]
fn web_yard_grant_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, granted) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::GrantYardAccess {
        scope: scope.clone(),
        yard: "site".into(),
        principal_kind: "user".into(),
        principal_id: "user_reader".into(),
        roles: vec!["viewer".into()],
        environment_id: Some("yardenv_site".into()),
        expires_at: Some("2100-01-01T00:00:00Z".into()),
    }))
    .expect("grant mapping");
    assert!(matches!(
        granted,
        Command::Access {
            command: AccessCommand::Grant(arguments)
        } if arguments.roles == ["viewer"]
            && arguments.environment.as_deref() == Some("yardenv_site")
            && arguments.expires.as_deref() == Some("2100-01-01T00:00:00Z")
    ));
    let (_, revoked) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::RevokeYardAccess {
        scope,
        yard: "site".into(),
        grant_id: "yardgrant_1".into(),
    }))
    .expect("revoke mapping");
    assert!(matches!(
        revoked,
        Command::Access {
            command: AccessCommand::Revoke(arguments)
        } if arguments.grant_id == "yardgrant_1"
    ));
}

#[test]
fn web_yard_session_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, listed) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListYardSessions {
        scope: scope.clone(),
        yard: "site".into(),
    }))
    .expect("session list mapping");
    assert!(matches!(
        listed,
        Command::YardSessions {
            command: YardSessionsCommand::List(arguments)
        } if arguments.name.as_deref() == Some("site")
    ));
    let (_, revoked) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::RevokeYardSession {
        scope,
        yard: "site".into(),
        session_id: "byys_session".into(),
    }))
    .expect("session revoke mapping");
    assert!(matches!(
        revoked,
        Command::YardSessions {
            command: YardSessionsCommand::Revoke(arguments)
        } if arguments.name == "site" && arguments.session_id == "byys_session"
    ));
}

#[test]
fn web_yard_management_role_list_and_set_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, listed) = mcp_yard_command(ToolCall::WebYard(
        WebYardToolCall::ListYardManagementRoles {
            scope: scope.clone(),
            yard: "site".into(),
            cursor: Some("next".into()),
        },
    ))
    .expect("management-role list mapping");
    assert!(matches!(
        listed,
        Command::ManagementRoles {
            command: ManagementRolesCommand::List(arguments)
        } if arguments.name == "site" && arguments.cursor.as_deref() == Some("next")
    ));
    let (_, set) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::SetYardManagementRole {
        scope,
        yard: "site".into(),
        user_id: "user_developer".into(),
        role: "developer".into(),
    }))
    .expect("management-role set mapping");
    assert!(matches!(
        set,
        Command::ManagementRoles {
            command: ManagementRolesCommand::Set(arguments)
        } if arguments.name == "site"
            && arguments.user_id == "user_developer"
            && arguments.role == "developer"
    ));
}

#[test]
fn web_yard_management_role_revoke_tool_maps_to_cli_contract() {
    let scope = Scope::default();
    let (_, revoked) = mcp_yard_command(ToolCall::WebYard(
        WebYardToolCall::RevokeYardManagementRole {
            scope,
            yard: "site".into(),
            user_id: "user_developer".into(),
        },
    ))
    .expect("management-role revoke mapping");
    assert!(matches!(
        revoked,
        Command::ManagementRoles {
            command: ManagementRolesCommand::Revoke(arguments)
        } if arguments.name == "site" && arguments.user_id == "user_developer"
    ));
}

#[test]
fn web_yard_policy_and_access_role_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, get_policy) = mcp_yard_command(ToolCall::WebYard(
        WebYardToolCall::GetYardApplicationPolicy {
            scope: scope.clone(),
            yard: "site".into(),
        },
    ))
    .expect("application-policy get mapping");
    assert!(matches!(
        get_policy,
        Command::ApplicationPolicy {
            command: ApplicationPolicyCommand::Get(arguments)
        } if arguments.name == "site"
    ));
    let (_, set_policy) = mcp_yard_command(ToolCall::WebYard(
        WebYardToolCall::SetYardApplicationPolicy {
            scope: scope.clone(),
            yard: "site".into(),
            source_manifest_digest: "a".repeat(64),
            default_role: Some("viewer".into()),
            roles: json!({
                "viewer": {
                    "inherits": [],
                    "permissions": ["content.read"]
                }
            }),
        },
    ))
    .expect("application-policy set mapping");
    assert!(matches!(
        set_policy,
        Command::ApplicationPolicy {
            command: ApplicationPolicyCommand::Set(arguments)
        } if arguments.name == "site"
            && arguments.source_manifest_digest == "a".repeat(64)
            && arguments.policy.is_none()
            && arguments.policy_json.as_deref().is_some_and(
                |value| value.contains("\"defaultRole\":\"viewer\"")
                    && value.contains("\"content.read\"")
            )
    ));
    let (_, set_roles) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::SetYardAccessRoles {
        scope,
        yard: "site".into(),
        grant_id: "yardgrant_reader".into(),
        roles: vec!["viewer".into()],
    }))
    .expect("application-role set mapping");
    assert!(matches!(
        set_roles,
        Command::Access {
            command: AccessCommand::SetRoles(arguments)
        } if arguments.name == "site"
            && arguments.grant_id == "yardgrant_reader"
            && arguments.roles == ["viewer"]
    ));
}

include!("mcp_yard_identity_edge_tests.rs");

#[path = "mcp_yard_guest_invite_tests.rs"]
mod guest_invite_tests;
