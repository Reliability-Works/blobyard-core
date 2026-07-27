#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{cursor, decoded, invitation, subject, validate_invitation, validate_subject};
use blobyard_contract::{
    RevocableStatus, YardAccessGrantRecord, YardAccessPrincipalKind, YardGuestInviteRecord,
    YardGuestInviteStatus, YardSubjectKind, YardSubjectRecord,
};
use rusqlite::Connection;

#[test]
fn row_value_decoders_reject_unknown_values() {
    assert!(decoded::<YardGuestInviteStatus>("unknown".to_owned(), None).is_err());
    assert!(decoded::<YardAccessPrincipalKind>("unknown".to_owned(), None).is_err());
    assert!(decoded::<RevocableStatus>("unknown".to_owned(), None).is_err());
    assert!(decoded::<YardSubjectKind>("unknown".to_owned(), None).is_err());
    assert!(decoded::<Vec<String>>("{".to_owned(), None).is_err());
}

#[test]
fn invitation_row_rejects_every_malformed_column_and_encoding() {
    let connection = Connection::open_in_memory().expect("connection");
    for column in [
        0_usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24,
    ] {
        let mut values = invitation_values();
        values[column] = if matches!(column, 10 | 11 | 12 | 13 | 21 | 23 | 24) {
            "'not-an-integer'"
        } else {
            "1"
        };
        assert!(
            map_invitation(&connection, &values).is_err(),
            "column {column} must reject the wrong SQLite type"
        );
    }
    for (column, value) in [
        (6, "'unknown'"),
        (9, "'{'"),
        (17, "'unknown'"),
        (20, "'unknown'"),
    ] {
        let mut values = invitation_values();
        values[column] = value;
        assert!(
            map_invitation(&connection, &values).is_err(),
            "column {column} must reject the unknown encoded value"
        );
    }
    for column in [10_usize, 11, 12, 13, 21, 23, 24] {
        let mut values = invitation_values();
        values[column] = "-1";
        assert!(
            map_invitation(&connection, &values).is_err(),
            "column {column} must reject a negative timestamp"
        );
    }
}

#[test]
fn subject_row_rejects_every_malformed_column_and_invalid_relationship() {
    let connection = Connection::open_in_memory().expect("connection");
    for column in 0_usize..=6 {
        let mut values = subject_values();
        values[column] = if matches!(column, 5 | 6) {
            "'not-an-integer'"
        } else {
            "1"
        };
        assert!(
            map_subject(&connection, &values).is_err(),
            "column {column} must reject the wrong SQLite type"
        );
    }
    for (column, value) in [(1, "'unknown'"), (5, "-1"), (6, "-1")] {
        let mut values = subject_values();
        values[column] = value;
        assert!(
            map_subject(&connection, &values).is_err(),
            "column {column} must reject the invalid value"
        );
    }
    let mut invalid_relationship = subject_values();
    invalid_relationship[3] = "'member_other'";
    assert!(map_subject(&connection, &invalid_relationship).is_err());
}

#[test]
fn guest_row_validation_rejects_invalid_text_and_covers_optional_revocation() {
    let mut invitation = valid_invitation();
    let grant = valid_grant();
    invitation.id.clear();
    assert!(validate_invitation(&invitation, &grant).is_err());

    let mut member = valid_subject();
    member.id.clear();
    assert!(validate_subject(&member).is_err());
    member.id = "member_fixture".to_owned();
    member.workspace_id.clear();
    assert!(validate_subject(&member).is_err());
    member.workspace_id = "workspace_fixture".to_owned();
    member.revoked_at_ms = Some(member.created_at_ms);
    assert!(validate_subject(&member).is_ok());

    let record = valid_invitation();
    assert_eq!(cursor(&record).id, record.id);
}

fn map_invitation(
    connection: &Connection,
    values: &[&str],
) -> rusqlite::Result<YardGuestInviteRecord> {
    connection.query_row(&format!("SELECT {}", values.join(", ")), [], invitation)
}

fn invitation_values() -> Vec<&'static str> {
    vec![
        "'ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        "'workspace_fixture'",
        "'project_fixture'",
        "'yard_fixture'",
        "'environment_fixture'",
        "'guest@example.test'",
        "'pending'",
        "NULL",
        "'yardgrant_fixture'",
        "'[]'",
        "1",
        "600001",
        "NULL",
        "NULL",
        "'yardgrant_fixture'",
        "'yard_fixture'",
        "'environment_fixture'",
        "'guest-invite'",
        "'ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        "'[]'",
        "'active'",
        "1",
        "'operator'",
        "600001",
        "NULL",
    ]
}

fn map_subject(connection: &Connection, values: &[&str]) -> rusqlite::Result<YardSubjectRecord> {
    connection.query_row(&format!("SELECT {}", values.join(", ")), [], subject)
}

fn subject_values() -> Vec<&'static str> {
    vec![
        "'member_fixture'",
        "'member'",
        "'workspace_fixture'",
        "'member_fixture'",
        "NULL",
        "1",
        "NULL",
    ]
}

fn valid_invitation() -> YardGuestInviteRecord {
    YardGuestInviteRecord {
        id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        environment_id: Some("environment_fixture".to_owned()),
        email: "guest@example.test".to_owned(),
        status: YardGuestInviteStatus::Pending,
        accepted_subject_id: None,
        grant_id: "yardgrant_fixture".to_owned(),
        app_roles: Vec::new(),
        created_at_ms: 1,
        expires_at_ms: 600_001,
        accepted_at_ms: None,
        revoked_at_ms: None,
    }
}

fn valid_grant() -> YardAccessGrantRecord {
    YardAccessGrantRecord {
        id: "yardgrant_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        environment_id: Some("environment_fixture".to_owned()),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        app_roles: Vec::new(),
        status: RevocableStatus::Active,
        created_at_ms: 1,
        created_by_principal: "operator".to_owned(),
        expires_at_ms: Some(600_001),
        revoked_at_ms: None,
    }
}

fn valid_subject() -> YardSubjectRecord {
    YardSubjectRecord {
        id: "member_fixture".to_owned(),
        kind: YardSubjectKind::Member,
        workspace_id: "workspace_fixture".to_owned(),
        local_user_id: Some("member_fixture".to_owned()),
        invitation_id: None,
        created_at_ms: 1,
        revoked_at_ms: None,
    }
}
