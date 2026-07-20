use super::{assert_each_column_rejects_blob, assert_replacements_fail, preview};

#[test]
fn rejects_every_malformed_column_status_and_timestamp() {
    let values = [
        "'preview'",
        "'workspace'",
        "'project'",
        "1000",
        "'active'",
        "1",
        "NULL",
    ];
    assert_each_column_rejects_blob(&values, preview);
    assert_replacements_fail(
        &values,
        [(3, "-1"), (4, "'invalid'"), (5, "-1"), (6, "-1")],
        preview,
    );
}
