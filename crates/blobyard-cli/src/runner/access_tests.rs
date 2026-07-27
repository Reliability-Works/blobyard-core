#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    access_lines, grant_line, parse_principal_kind, parse_visibility, principal_kind_label,
    visibility_label,
};
use crate::TokenStore;
use crate::runner::login::tests::support::{
    Fixture, api_failure, fixture_tokens, fixture_yards, ok,
};
use blobyard_api_client::{
    Endpoint, GrantYardAccessPrincipalKind, YardAccessGrantSummary, YardAccessPrincipalKind,
    YardAccessResponse, YardVisibility,
};
use blobyard_core::{ErrorCode, SecretString};
use serde_json::json;

fn grant(id: &str, environment_id: Option<&str>, roles: &[&str]) -> YardAccessGrantSummary {
    serde_json::from_value(serde_json::json!({
        "appRoles": roles,
        "createdAt": "1970-01-01T00:00:00.001Z",
        "environmentId": environment_id,
        "expiresAt": null,
        "id": id,
        "principalId": "user_docs",
        "principalKind": "user"
    }))
    .expect("grant")
}

#[test]
fn access_lines_cover_empty_and_populated_grant_lists() {
    let empty = YardAccessResponse {
        grants: vec![],
        visibility: YardVisibility::Public,
    };
    assert_eq!(
        access_lines(&empty),
        "visibility\tpublic\nNo active grants."
    );
    let populated = YardAccessResponse {
        grants: vec![
            grant("yardgrant_a", Some("yardenv_yard"), &["editor", "viewer"]),
            grant("yardgrant_b", None, &[]),
        ],
        visibility: YardVisibility::Owner,
    };
    assert_eq!(
        access_lines(&populated),
        "visibility\towner\n\
         user\tuser_docs\tyardenv_yard\troles editor,viewer\texpires never\tyardgrant_a\n\
         user\tuser_docs\tall-environments\troles none\texpires never\tyardgrant_b"
    );
}

#[test]
fn grant_lines_show_expiry_timestamps() {
    let mut expiring = grant("yardgrant_c", None, &["viewer"]);
    expiring.expires_at = Some("1970-01-01T00:00:01Z".to_owned());
    assert_eq!(
        grant_line(&expiring),
        "user\tuser_docs\tall-environments\troles viewer\texpires 1970-01-01T00:00:01Z\tyardgrant_c"
    );
}

#[test]
fn visibility_parsing_round_trips_and_rejects_unknown_values() {
    for visibility in [
        YardVisibility::Public,
        YardVisibility::Owner,
        YardVisibility::Selected,
        YardVisibility::Workspace,
        YardVisibility::AuthenticatedLink,
        YardVisibility::AnyAuthenticated,
    ] {
        assert_eq!(
            parse_visibility(visibility_label(visibility)).expect("visibility"),
            visibility
        );
    }
    assert_eq!(
        parse_visibility("hidden")
            .expect_err("unknown visibility")
            .code(),
        ErrorCode::InvalidRequest
    );
}

#[test]
fn principal_kind_parsing_round_trips_and_rejects_unknown_values() {
    for (label, kind) in [
        ("user", GrantYardAccessPrincipalKind::User),
        ("group", GrantYardAccessPrincipalKind::Group),
        ("link", GrantYardAccessPrincipalKind::Link),
    ] {
        assert_eq!(parse_principal_kind(label).expect("principal kind"), kind);
    }
    for (kind, label) in [
        (YardAccessPrincipalKind::User, "user"),
        (YardAccessPrincipalKind::Group, "group"),
        (YardAccessPrincipalKind::GuestInvite, "guest-invite"),
        (YardAccessPrincipalKind::Link, "link"),
    ] {
        assert_eq!(principal_kind_label(kind), label);
    }
    assert_eq!(
        parse_principal_kind("robot")
            .expect_err("unknown principal kind")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        parse_principal_kind("guest-invite")
            .expect_err("dedicated guest invitation path")
            .code(),
        ErrorCode::InvalidRequest
    );
}

#[tokio::test]
async fn set_access_roles_executes_the_typed_api_contract() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "access",
            "set-roles",
            "documentation",
            "yardgrant_reader",
            "--role",
            "viewer",
            "--role",
            "editor",
        ],
        vec![
            fixture_tokens(),
            fixture_yards(),
            fixture_tokens(),
            ok(
                &json!({
                    "grant": {
                        "appRoles": ["viewer", "editor"],
                        "createdAt": "2026-07-24T09:00:00Z",
                        "environmentId": null,
                        "expiresAt": null,
                        "id": "yardgrant_reader",
                        "principalId": "user_reader",
                        "principalKind": "user"
                    }
                }),
                "request_set_roles",
            ),
        ],
    );
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("set Yard application roles");
    assert_eq!(
        result.into_data()["grant"]["appRoles"],
        json!(["viewer", "editor"])
    );
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[3].endpoint(), Endpoint::SetYardAccessRoles);
    assert_eq!(
        requests[3].body(),
        Some(&json!({
            "yardId": "yard_documentation",
            "grantId": "yardgrant_reader",
            "appRoles": ["viewer", "editor"]
        }))
    );
}

#[tokio::test]
async fn set_access_roles_propagates_selection_and_api_failures() {
    for (name, responses, expected) in [
        (
            "missing-yard",
            vec![fixture_tokens(), fixture_yards()],
            ErrorCode::NotFound,
        ),
        (
            "documentation",
            vec![
                fixture_tokens(),
                fixture_yards(),
                fixture_tokens(),
                api_failure(ErrorCode::Conflict, 409, "request_set_roles_failed"),
            ],
            ErrorCode::Conflict,
        ),
    ] {
        let fixture = Fixture::new(
            &[
                "blobyard",
                "--workspace",
                "main",
                "--project",
                "artifacts",
                "access",
                "set-roles",
                name,
                "yardgrant_reader",
                "--role",
                "viewer",
            ],
            responses,
        );
        fixture
            .store
            .save(&SecretString::new("local-api-token").expect("token"))
            .expect("store token");
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("set roles failure")
                .code(),
            expected
        );
    }
}
