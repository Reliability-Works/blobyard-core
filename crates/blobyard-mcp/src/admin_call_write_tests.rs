#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::parse;
use super::{AdminToolCall, Scope, arguments, parse_admin_call};
use serde_json::json;

#[test]
fn parses_credential_administration_writes() {
    assert_eq!(
        parse(
            "deactivate_local_user",
            &json!({ "confirm": true, "user_id": "user_1" }),
        ),
        AdminToolCall::DeactivateLocalUser {
            scope: Scope::default(),
            user_id: "user_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "revoke_api_token",
            &json!({ "confirm": true, "token_id": "token_1" }),
        ),
        AdminToolCall::RevokeApiToken {
            scope: Scope::default(),
            token_id: "token_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "revoke_ci_trust",
            &json!({ "confirm": true, "trust_id": "trust_1" }),
        ),
        AdminToolCall::RevokeCiTrust {
            scope: Scope::default(),
            trust_id: "trust_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "revoke_cli_session",
            &json!({ "confirm": true, "session_id": "session_1" }),
        ),
        AdminToolCall::RevokeCliSession {
            scope: Scope::default(),
            session_id: "session_1".to_owned(),
            confirmed: true,
        }
    );
}

#[test]
fn parses_ci_trust_with_optional_environment() {
    let required = json!({
        "allowed_actions": ["upload", "share"],
        "allowed_ref_glob": "refs/heads/main",
        "repository": "acme/artifacts",
        "workflow_path": ".github/workflows/upload-artifacts.yml",
        "workflow_ref": "refs/heads/main"
    });
    let parsed = parse("create_ci_trust", &required);
    assert!(matches!(
        parsed,
        AdminToolCall::CreateCiTrust {
            environment: None,
            ..
        }
    ));
    let mut with_environment = arguments(&required);
    with_environment.insert("environment".to_owned(), json!("Production"));
    assert!(matches!(
        parse_admin_call("create_ci_trust", &with_environment, Scope::default()),
        Ok(AdminToolCall::CreateCiTrust {
            allowed_actions,
            environment: Some(environment),
            ..
        }) if allowed_actions == ["upload", "share"] && environment == "Production"
    ));
}
