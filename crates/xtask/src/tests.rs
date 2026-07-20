#![allow(
    clippy::expect_used,
    reason = "isolated filesystem fixtures must be created"
)]

use super::{
    MAX_RUST_FILE_LINES, check_limits, classify_path, collect_entries, count_source_lines,
};
use std::fs;
use std::io;

#[test]
fn counts_only_lines_with_source_tokens() {
    let source = r#"
        // comment
        /* outer
           /* nested */
        */
        fn example() { // inline comment
            let marker = "// this string is code";
        }
    "#;

    assert_eq!(count_source_lines(source), 3);
}

#[test]
fn propagates_iterator_and_file_type_errors() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let source = directory.path().join("sample.rs");
    fs::write(&source, "fn sample() {}\n").expect("sample source must be written");
    let entries = fs::read_dir(directory.path())
        .expect("temporary directory must be readable")
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    collect_entries(entries, &mut files).expect("materialized entries must be collected");
    assert_eq!(files, [source]);

    let iterator_error = io::Error::other("entry failed");
    let entries: Vec<io::Result<fs::DirEntry>> = vec![Err(iterator_error)];
    let error = collect_entries(entries, &mut files).expect_err("entry error must propagate");
    assert_eq!(error.kind(), io::ErrorKind::Other);

    let type_error = io::Error::other("file type failed");
    let error = classify_path("missing.rs".into(), Err(type_error), &mut files)
        .expect_err("file type error must propagate");
    assert_eq!(error.kind(), io::ErrorKind::Other);
}

#[test]
fn propagates_nested_directory_errors() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let nested = directory.path().join("removed");
    fs::create_dir(&nested).expect("nested directory must be created");
    let file_type = fs::metadata(&nested)
        .expect("nested directory metadata must exist")
        .file_type();
    fs::remove_dir(&nested).expect("nested directory must be removable");

    let error = classify_path(nested, Ok(file_type), &mut Vec::new())
        .expect_err("removed nested directory must fail traversal");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn propagates_root_and_source_read_errors() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let error = check_limits(directory.path()).expect_err("missing crates directory must fail");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);

    let source = directory.path().join("crates/example/src");
    fs::create_dir_all(&source).expect("source directory must be created");
    fs::write(source.join("invalid.rs"), [0xff]).expect("invalid UTF-8 fixture must be written");
    let error = check_limits(directory.path()).expect_err("invalid UTF-8 source must fail");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn reports_only_rust_files_over_the_limit() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let source = directory.path().join("crates/example/src");
    fs::create_dir_all(&source).expect("source directory must be created");
    fs::write(source.join("short.rs"), "fn short() {}\n").expect("short source must be written");
    fs::write(source.join("notes.txt"), "ignored\n").expect("non-Rust file must be written");
    let oversized = "const VALUE: usize = 1;\n".repeat(MAX_RUST_FILE_LINES + 1);
    fs::write(source.join("long.rs"), oversized).expect("long source must be written");

    let violations = check_limits(directory.path()).expect("limit scan must succeed");
    assert_eq!(violations.len(), 1);
    assert_eq!(
        violations[0].path().to_string_lossy(),
        "crates/example/src/long.rs"
    );
    assert_eq!(violations[0].actual_lines(), MAX_RUST_FILE_LINES + 1);
    assert_eq!(violations[0].maximum_lines(), MAX_RUST_FILE_LINES);
}
