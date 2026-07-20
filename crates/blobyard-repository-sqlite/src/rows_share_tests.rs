use super::{assert_each_column_rejects_blob, assert_replacements_fail, share};

#[test]
fn rejects_every_malformed_column_status_and_timestamp() {
    let values = [
        "'share'",
        "'workspace'",
        "'version'",
        "1000",
        "'active'",
        "0",
        "1",
        "1",
        "NULL",
    ];
    assert_each_column_rejects_blob(&values, share);
    assert_replacements_fail(
        &values,
        [
            (3, "-1"),
            (4, "'invalid'"),
            (5, "-1"),
            (6, "-1"),
            (7, "-1"),
            (8, "-1"),
        ],
        share,
    );
}
