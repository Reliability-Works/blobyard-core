#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    access_lines, grant_line, parse_principal_kind, parse_visibility, principal_kind_label,
    visibility_label,
};
use blobyard_api_client::{
    YardAccessGrantSummary, YardAccessPrincipalKind, YardAccessResponse, YardVisibility,
};
use blobyard_core::ErrorCode;

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
    for kind in [
        YardAccessPrincipalKind::User,
        YardAccessPrincipalKind::Group,
        YardAccessPrincipalKind::GuestInvite,
        YardAccessPrincipalKind::Link,
    ] {
        assert_eq!(
            parse_principal_kind(principal_kind_label(kind)).expect("principal kind"),
            kind
        );
    }
    assert_eq!(
        parse_principal_kind("robot")
            .expect_err("unknown principal kind")
            .code(),
        ErrorCode::InvalidRequest
    );
}
