#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    api_token, ci_actions, ci_trust, cli_session, conversion_error, machine_session,
    object_version, preview, project, share, stored_object, upload_part, upload_reservation,
    workspace,
};
use rusqlite::{Connection, Row};

#[path = "rows_preview_tests.rs"]
mod previews;
#[path = "rows_share_tests.rs"]
mod sharing;

type Decoder<T> = fn(&Row<'_>) -> rusqlite::Result<T>;

pub(in crate::adapter) fn assert_each_column_rejects_blob<T>(values: &[&str], decode: Decoder<T>) {
    let connection = Connection::open_in_memory().expect("database");
    for index in 0..values.len() {
        let mut corrupted = values.to_vec();
        corrupted[index] = "x'00'";
        let sql = format!("SELECT {}", corrupted.join(", "));
        assert!(
            connection.query_row(&sql, [], decode).is_err(),
            "column {index}"
        );
    }
}

fn assert_query_fails<T>(values: &[&str], decode: Decoder<T>) {
    let connection = Connection::open_in_memory().expect("database");
    let sql = format!("SELECT {}", values.join(", "));
    assert!(connection.query_row(&sql, [], decode).is_err());
}

pub(in crate::adapter) fn assert_replacements_fail<T, const N: usize>(
    values: &[&str],
    replacements: [(usize, &str); N],
    decode: Decoder<T>,
) {
    for (index, replacement) in replacements {
        let mut corrupted = values.to_vec();
        corrupted[index] = replacement;
        assert_query_fails(&corrupted, decode);
    }
}

#[test]
fn namespace_and_token_rows_reject_every_malformed_column() {
    assert_each_column_rejects_blob(&["'workspace'", "'Workspace'", "'workspace'"], workspace);
    assert_each_column_rejects_blob(
        &["'project'", "'workspace'", "'Project'", "'project'"],
        project,
    );
    assert_each_column_rejects_blob(
        &[
            "'token'",
            "'Token'",
            "'hash'",
            "'read\\nwrite'",
            "'workspace'",
            "'byd_pat_fixture'",
            "NULL",
            "1",
            "1000",
            "NULL",
            "NULL",
        ],
        api_token,
    );
    assert_replacements_fail(
        &[
            "'token'",
            "'Token'",
            "'hash'",
            "'read\\nwrite'",
            "'workspace'",
            "'byd_pat_fixture'",
            "NULL",
            "1",
            "1000",
            "2",
            "3",
        ],
        [(7, "-1"), (8, "-1"), (9, "-1"), (10, "-1")],
        api_token,
    );
    assert_query_fails(&["'workspace'", "'Workspace'", "'bad slug'"], workspace);
    assert_query_fails(
        &["'project'", "'workspace'", "'Project'", "'bad slug'"],
        project,
    );
}

#[test]
fn cli_session_rows_reject_every_malformed_column_and_timestamp() {
    let values = [
        "'session'",
        "'token'",
        "'workspace'",
        "'Work laptop'",
        "'macos'",
        "'0.1.12'",
        "1",
        "2",
        "NULL",
    ];
    assert_each_column_rejects_blob(&values, cli_session);
    assert_replacements_fail(&values, [(6, "-1"), (7, "-1"), (8, "-1")], cli_session);
}

#[test]
fn ci_rows_reject_every_malformed_column_action_and_timestamp() {
    let trust = [
        "'trust'",
        "'workspace'",
        "'project'",
        "'owner/repository'",
        "'.github/workflows/release.yml'",
        "'refs/heads/main'",
        "'refs/heads/*'",
        "NULL",
        "'upload\ndownload'",
        "'https://api.example'",
        "1",
        "NULL",
    ];
    assert_each_column_rejects_blob(&trust, ci_trust);
    assert_replacements_fail(&trust, [(8, "'invalid'"), (10, "-1"), (11, "-1")], ci_trust);

    let session = [
        "'machine_fixture'",
        "'trust'",
        "'workspace'",
        "'project'",
        "'owner/repository'",
        "'refs/heads/main'",
        "'12345'",
        "'1'",
        "'upload'",
        "1",
        "1000",
        "NULL",
        "NULL",
    ];
    assert_each_column_rejects_blob(&session, machine_session);
    assert_replacements_fail(
        &session,
        [
            (8, "'invalid'"),
            (9, "-1"),
            (10, "-1"),
            (11, "-1"),
            (12, "-1"),
        ],
        machine_session,
    );
    for value in ["", "invalid", "upload\ninvalid"] {
        assert!(ci_actions(value).is_err());
    }
    assert_eq!(
        ci_actions("upload\ndownload").map(|actions| actions.len()),
        Ok(2)
    );
}

#[test]
fn object_version_rows_reject_every_malformed_column_and_domain_value() {
    let values = [
        "'version'",
        "'project'",
        "'artifact.bin'",
        "1",
        "'objects/version'",
        "'complete'",
        "5",
        "'checksum'",
        "10",
        "'ci'",
        "'example/core-project'",
        "'0123456789abcdef'",
        "'main'",
    ];
    assert_each_column_rejects_blob(&values, object_version);

    assert_replacements_fail(
        &values,
        [
            (3, "-1"),
            (5, "'invalid'"),
            (6, "-1"),
            (8, "-1"),
            (9, "'invalid'"),
        ],
        object_version,
    );
}

#[test]
fn reservation_rows_reject_every_malformed_column_and_domain_value() {
    let values = [
        "'reservation'",
        "'version'",
        "'project'",
        "'artifact.bin'",
        "1",
        "'objects/version'",
        "'pending'",
        "NULL",
        "NULL",
        "10",
        "'ci'",
        "'example/core-project'",
        "'0123456789abcdef'",
        "'main'",
        "'artifact.bin'",
        "'application/octet-stream'",
        "5",
        "'checksum'",
        "20",
        "'requested'",
        "'multipart'",
        "3",
        "2",
        "'provider'",
    ];
    assert_each_column_rejects_blob(&values, upload_reservation);

    assert_replacements_fail(
        &values,
        [
            (4, "-1"),
            (6, "'invalid'"),
            (7, "-1"),
            (9, "-1"),
            (10, "'invalid'"),
            (16, "-1"),
            (18, "-1"),
            (19, "'invalid'"),
            (20, "'invalid'"),
            (21, "-1"),
            (22, "-1"),
        ],
        upload_reservation,
    );
}

#[test]
fn upload_part_rows_reject_every_malformed_column_and_timestamp() {
    let values = ["'upload'", "1", "3", "20", "3", "'checksum'"];
    assert_each_column_rejects_blob(&values, upload_part);
    assert_replacements_fail(
        &values,
        [(1, "-1"), (2, "-1"), (3, "-1"), (4, "-1")],
        upload_part,
    );
}

#[test]
fn stored_object_rows_reject_every_malformed_column_and_domain_value() {
    let values = [
        "'version'",
        "'project'",
        "'artifact.bin'",
        "1",
        "'objects/version'",
        "'complete'",
        "5",
        "'checksum'",
        "10",
        "'ci'",
        "'example/core-project'",
        "'0123456789abcdef'",
        "'main'",
        "'artifact.bin'",
        "'application/octet-stream'",
    ];
    assert_each_column_rejects_blob(&values, stored_object);

    assert_replacements_fail(
        &values,
        [
            (3, "-1"),
            (5, "'invalid'"),
            (6, "-1"),
            (8, "-1"),
            (9, "'invalid'"),
        ],
        stored_object,
    );
}

#[test]
fn conversion_failures_preserve_safe_debug_context() {
    let error = conversion_error("invalid fixture");
    let context = match error {
        rusqlite::Error::FromSqlConversionFailure(_, _, source) => Some(source.to_string()),
        _ => None,
    };
    assert_eq!(context.as_deref(), Some("\"invalid fixture\""));
}
