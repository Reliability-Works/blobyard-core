#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::TokenStore;
use crate::commands::{McpCommand, McpServeArgs, YardCommand};
use blobyard_core::SecretString;
use blobyard_mcp::WebYardToolCall;

#[test]
fn zero_retention_count_fails_before_command_execution() {
    let call = ToolCall::SetRetention {
        scope: Scope::default(),
        latest: 0,
        branch: None,
        path: None,
    };
    assert_eq!(
        mcp_command(call).expect_err("zero").code(),
        ErrorCode::InvalidRequest
    );
}

#[test]
fn category_mappers_fail_closed_for_wrong_call_kinds() {
    let wrong = ToolCall::ClearRetention {
        scope: Scope::default(),
    };
    assert!(mcp_resource_command(wrong.clone()).is_err());
    assert!(mcp_transfer_command(wrong).is_err());
    assert!(
        super::super::mcp_yards::mcp_yard_command(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .is_err()
    );
    assert!(
        mcp_retention_command(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .is_err()
    );
    assert!(
        mcp_capability_command(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .is_err()
    );
    assert!(
        mcp_command(ToolCall::Admin(blobyard_mcp::AdminToolCall::ListMembers {
            scope: Scope::default(),
        }))
        .is_err()
    );
}

#[test]
fn agent_object_and_preview_calls_map_to_existing_cli_contracts() {
    let scope = Scope::default();
    let (_, delete) = mcp_command(ToolCall::DeleteObject {
        scope: scope.clone(),
        uri: "blobyard://team/site/index.html".to_owned(),
    })
    .expect("delete mapping");
    assert!(matches!(delete, Command::Rm(_)));
    let (_, preview) = mcp_command(ToolCall::CreatePreview {
        scope,
        directory: "./site".to_owned(),
        expires: Some("7d".to_owned()),
    })
    .expect("preview mapping");
    assert!(matches!(preview, Command::Preview(_)));
}

#[test]
fn dashboard_tools_map_to_safe_cli_contracts_without_deletion_tokens() {
    let scope = Scope::default();
    let (_, rename) = mcp_command(ToolCall::Dashboard(
        blobyard_mcp::DashboardToolCall::RenameWorkspace {
            scope: scope.clone(),
            name: "Product".into(),
        },
    ))
    .expect("rename mapping");
    assert!(matches!(rename, Command::Workspaces { .. }));

    for call in [
        blobyard_mcp::DashboardToolCall::RequestAccountExport {
            scope: scope.clone(),
        },
        blobyard_mcp::DashboardToolCall::GetBilling {
            scope: scope.clone(),
        },
        blobyard_mcp::DashboardToolCall::GetAccountExport {
            scope: scope.clone(),
        },
        blobyard_mcp::DashboardToolCall::GetAccountDeletion {
            scope: scope.clone(),
        },
        blobyard_mcp::DashboardToolCall::GetRetentionOverview { scope },
    ] {
        let (_, command) = mcp_command(ToolCall::Dashboard(call)).expect("dashboard mapping");
        assert!(matches!(
            command,
            Command::Billing { .. } | Command::Account { .. } | Command::Retention { .. }
        ));
    }

    assert!(
        super::super::mcp_dashboard::mcp_dashboard_command(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .is_err()
    );
}

#[test]
fn workspace_resource_calls_map_to_headless_commands() {
    let scope = Scope::default();
    let (_, list) = mcp_command(ToolCall::ListWorkspaces {
        scope: scope.clone(),
    })
    .expect("list workspaces mapping");
    assert!(matches!(list, Command::Workspaces { .. }));
    let (_, create) = mcp_command(ToolCall::CreateWorkspace {
        scope,
        name: "Platform".to_owned(),
    })
    .expect("create workspace mapping");
    assert!(matches!(create, Command::Workspaces { .. }));
}

#[test]
fn web_yard_tools_map_to_confirmed_cli_contracts() {
    let scope = Scope::default();
    let (_, deploy) = mcp_command(ToolCall::WebYard(WebYardToolCall::DeployWebYard {
        scope: scope.clone(),
        directory: "./dist".into(),
        yard: "site".into(),
        spa: true,
        clean_urls: false,
    }))
    .expect("deploy mapping");
    assert!(matches!(deploy, Command::Deploy(arguments) if arguments.public));
    let (_, delete) = mcp_command(ToolCall::WebYard(WebYardToolCall::DeleteWebYard {
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
    let (_, list) = mcp_command(ToolCall::WebYard(WebYardToolCall::ListWebYards {
        scope: scope.clone(),
    }))
    .expect("list mapping");
    assert!(matches!(
        list,
        Command::Yard {
            command: YardCommand::List
        }
    ));
    let (_, history) = mcp_command(ToolCall::WebYard(WebYardToolCall::ListYardDeploys {
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
    let (_, rollback) = mcp_command(ToolCall::WebYard(WebYardToolCall::RollbackWebYard {
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

#[tokio::test]
async fn invalid_tool_mapping_and_stdio_errors_fail_closed() {
    let fixture =
        super::super::login::tests::support::Fixture::new(&["blobyard", "whoami"], vec![]);
    let invalid = ToolCall::SetRetention {
        scope: Scope::default(),
        latest: 0,
        branch: None,
        path: None,
    };
    assert!(fixture.runner.execute_mcp(invalid).await.is_err());
    assert!(
        fixture
            .runner
            .execute(&Command::Mcp {
                command: McpCommand::Serve(McpServeArgs { stdio: true }),
            })
            .await
            .is_err()
    );
    assert!(finish_mcp(Ok(())).is_ok());
    let error = finish_mcp(Err(std::io::Error::other("synthetic"))).expect_err("stdio error");
    assert_eq!(error.code(), ErrorCode::InternalError);
}

#[tokio::test]
async fn administration_calls_use_scoped_versioned_endpoints() {
    use super::super::login::tests::support::{Fixture, ok};

    let fixture = Fixture::new(
        &["blobyard", "--workspace", "main", "whoami"],
        vec![
            ok(
                &serde_json::json!({
                    "accessToken": "access-token-fixture",
                    "refreshToken": "next-refresh-token-fixture",
                    "expiresInSeconds": 900
                }),
                "req_refresh",
            ),
            ok(&serde_json::json!({ "members": [] }), "req_members"),
        ],
    );
    fixture
        .store
        .save(&SecretString::new("refresh-token-fixture").expect("valid test token"))
        .expect("save test token");
    let result = fixture
        .runner
        .execute_mcp(ToolCall::Admin(blobyard_mcp::AdminToolCall::ListMembers {
            scope: Scope::default(),
        }))
        .await
        .expect("administration call");
    assert_eq!(result, serde_json::json!({ "members": [] }));
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].endpoint(), Endpoint::ListMembers);
    assert_eq!(requests[1].query(), Some("workspace=main"));
}

#[tokio::test]
async fn administration_execution_propagates_scope_request_and_remote_failures() {
    use super::super::login::tests::support::{Fixture, api_failure, ok};

    let invalid_scope = Fixture::new(&["blobyard", "whoami"], vec![]);
    assert!(
        invalid_scope
            .runner
            .execute_mcp(ToolCall::Admin(blobyard_mcp::AdminToolCall::ListMembers {
                scope: Scope {
                    workspace: Some("INVALID SPACE".to_owned()),
                    project: None,
                },
            }))
            .await
            .is_err()
    );

    let missing_workspace = Fixture::new(&["blobyard", "whoami"], vec![]);
    assert!(
        missing_workspace
            .runner
            .execute_mcp(ToolCall::Admin(blobyard_mcp::AdminToolCall::ListMembers {
                scope: Scope::default(),
            }))
            .await
            .is_err()
    );

    let remote_failure = Fixture::new(
        &["blobyard", "--workspace", "main", "whoami"],
        vec![
            ok(
                &serde_json::json!({
                    "accessToken": "access-token-fixture",
                    "refreshToken": "next-refresh-token-fixture",
                    "expiresInSeconds": 900
                }),
                "req_refresh_admin_failure",
            ),
            api_failure(ErrorCode::InternalError, 500, "req_admin_failure"),
        ],
    );
    remote_failure
        .store
        .save(&SecretString::new("refresh-token-fixture").expect("valid test token"))
        .expect("save test token");
    assert!(
        remote_failure
            .runner
            .execute_mcp(ToolCall::Admin(blobyard_mcp::AdminToolCall::ListMembers {
                scope: Scope::default(),
            }))
            .await
            .is_err()
    );
}
