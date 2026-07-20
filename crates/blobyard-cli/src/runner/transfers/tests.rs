#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::discovery::{
    collect_entry, default_excluded, directory_name, directory_prefix_with, discover, file_entry,
    portable_path, relative_file, validate_logical,
};
use super::file_facts::{content_type, fingerprint, inspect, inspect_with_hook, validate_measured};
use super::resume::{ResumeState, load, remove, save, state_path};
use super::upload_math::{part_range, total_parts, validate_grants};
use crate::commands::UploadArgs;
use blobyard_api_client::UploadPartGrant;
use blobyard_core::{ErrorCode, SecretString};
use std::path::{Path, PathBuf};

#[tokio::test]
async fn discovery_facts_and_resume_state_are_stable() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("tree");
    std::fs::create_dir_all(root.join("nested")).expect("dirs");
    std::fs::create_dir_all(root.join("node_modules")).expect("ignored dir");
    std::fs::write(root.join("nested/app.json"), b"{}").expect("file");
    std::fs::write(root.join("skip.log"), b"skip").expect("ignored file");
    std::fs::write(root.join(".blobyardignore"), b"skip.log\n").expect("ignore");
    std::fs::write(root.join("node_modules/package.js"), b"bad").expect("default");
    let files = discover(&UploadArgs {
        source: root.clone(),
        path: Some("builds".into()),
        include_ignored: false,
    })
    .expect("discovery");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].logical_path, "builds/nested/app.json");
    let facts = inspect(&files[0].source).await.expect("facts");
    assert_eq!(facts.size_bytes, 2);
    assert_eq!(facts.content_type, "application/json");
    assert_eq!(
        content_type(PathBuf::from("unknown").as_path()),
        "application/octet-stream"
    );
    assert_eq!(fingerprint(1, 2, "abc").len(), 64);
    assert_eq!(blobyard_core::hex_digest(&[0, 255]), "00ff");

    let path = state_path(&files[0].source, &files[0].logical_path);
    let mut state = ResumeState::new(
        "upload_1".into(),
        facts.fingerprint.clone(),
        8 * 1024 * 1024,
    );
    assert!(state.matches(&facts.fingerprint));
    state.record(2, "etag-2".into());
    state.record(1, "etag-1".into());
    assert_eq!(state.pending(3), vec![3]);
    assert_eq!(
        state.completed_bytes(8 * 1024 * 1024 + 3),
        8 * 1024 * 1024 + 3
    );
    state.retain_server_parts(&[2]);
    assert_eq!(state.pending(3), vec![1, 3]);
    assert_eq!(state.completed_bytes(8 * 1024 * 1024 + 3), 3);
    save(&path, &state).expect("save");
    assert_eq!(load(&path).expect("load"), Some(state));
    remove(&path).expect("remove");
    remove(&path).expect("idempotent remove");
    assert_eq!(load(&path).expect("missing"), None);
}

#[test]
fn multipart_math_and_grants_fail_closed() {
    let part_size = 8 * 1024 * 1024;
    assert_eq!(total_parts(part_size + 1, part_size).expect("parts"), 2);
    assert!(total_parts(0, part_size).is_err());
    assert!(total_parts(1, 0).is_err());
    assert!(total_parts(part_size * 10_001, part_size).is_err());
    assert_eq!(part_range(2, 8, 10), (8, 2));
    let grants = vec![grant(2), grant(1)];
    assert_eq!(
        validate_grants(&[1, 2], grants).expect("grants")[0].part_number,
        1
    );
    assert!(validate_grants(&[1], vec![grant(2)]).is_err());
}

fn grant(number: u32) -> UploadPartGrant {
    UploadPartGrant {
        part_number: number,
        upload_url: SecretString::new(format!("https://invalid/{number}")).expect("url"),
    }
}

#[test]
fn logical_paths_defaults_and_mime_types_cover_supported_inputs() {
    for invalid in [
        String::new(),
        "x".repeat(2_049),
        "/root".into(),
        "tail/".into(),
        "bad\\path".into(),
        "line\nbreak".into(),
        "a//b".into(),
        "a/./b".into(),
        "a/../b".into(),
    ] {
        assert!(validate_logical(&invalid).is_err());
    }
    assert_eq!(portable_path(Path::new("a/b")).expect("portable"), "a/b");
    assert!(portable_path(Path::new("../unsafe")).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_vec(vec![0xff]);
        assert!(portable_path(Path::new(&invalid)).is_err());
    }
    for excluded in [
        ".git",
        "node_modules",
        ".next",
        "target",
        ".cache",
        ".turbo",
        "__pycache__",
        ".blobyard-resume-123.json",
    ] {
        assert!(default_excluded(excluded));
    }
    assert!(!default_excluded("artifact"));
    let mime_cases = [
        ("x.html", "text/html; charset=utf-8"),
        ("x.css", "text/css; charset=utf-8"),
        ("x.js", "text/javascript; charset=utf-8"),
        ("x.json", "application/json"),
        ("x.md", "text/plain; charset=utf-8"),
        ("x.png", "image/png"),
        ("x.jpeg", "image/jpeg"),
        ("x.gif", "image/gif"),
        ("x.svg", "image/svg+xml"),
        ("x.pdf", "application/pdf"),
        ("x.zip", "application/zip"),
        ("x.gz", "application/gzip"),
        ("x.wasm", "application/wasm"),
    ];
    for (name, expected) in mime_cases {
        assert_eq!(content_type(Path::new(name)), expected);
    }
}

#[tokio::test]
async fn discovery_rejects_unsafe_sources() {
    let temp = tempfile::tempdir().expect("temp");
    let missing = UploadArgs {
        source: temp.path().join("missing"),
        path: None,
        include_ignored: false,
    };
    assert_eq!(
        discover(&missing).expect_err("missing").code(),
        ErrorCode::StorageError
    );
    let empty = temp.path().join("empty");
    std::fs::create_dir(&empty).expect("empty");
    assert!(discover(&upload_args(empty.clone(), false)).is_err());
    let invalid_prefix = UploadArgs {
        source: empty.clone(),
        path: Some("../unsafe".into()),
        include_ignored: false,
    };
    assert!(discover(&invalid_prefix).is_err());
    let canonical_failure = directory_prefix_with(&upload_args(empty.clone(), false), |_path| {
        Err(std::io::Error::other("synthetic canonicalization failure"))
    });
    assert!(canonical_failure.is_err());

    let file = temp.path().join("file.bin");
    std::fs::write(&file, b"x").expect("file");
    assert_eq!(
        discover(&upload_args(file.clone(), false))
            .expect("file")
            .len(),
        1
    );
    let entry = ignore::WalkBuilder::new(temp.path())
        .build()
        .filter_map(Result::ok)
        .find(|entry| entry.path() == file)
        .expect("walked file");
    let wrong_root = upload_args(temp.path().join("other-root"), false);
    assert!(collect_entry(&wrong_root, &entry, &mut Vec::new()).is_err());
    let relative = relative_file(&upload_args(temp.path().to_path_buf(), false), &file)
        .expect("unprefixed helper");
    assert_eq!(relative.logical_path, "file.bin");
    let invalid_relative = UploadArgs {
        source: temp.path().to_path_buf(),
        path: Some("../unsafe".into()),
        include_ignored: false,
    };
    assert!(relative_file(&invalid_relative, &file).is_err());
    assert!(file_entry(&file, Some("../unsafe")).is_err());
    let unsafe_child = wrong_root.source.join("../unsafe");
    assert!(relative_file(&wrong_root, &unsafe_child).is_err());
    assert!(inspect(&empty).await.is_err());
    assert!(inspect(&missing.source).await.is_err());
    assert!(validate_measured(1, 2).is_err());
    assert!(validate_measured(2, 2).is_ok());
}

#[test]
fn discovery_can_explicitly_include_ignored_files() {
    let temp = tempfile::tempdir().expect("temp");
    let tree = temp.path().join("ignored");
    std::fs::create_dir_all(tree.join("node_modules")).expect("tree");
    std::fs::write(tree.join(".gitignore"), b"ignored.txt\n").expect("gitignore");
    std::fs::write(tree.join("ignored.txt"), b"included").expect("ignored");
    std::fs::write(tree.join("node_modules/pkg.js"), b"included").expect("default");

    assert_eq!(
        discover(&upload_args(tree, true)).expect("included").len(),
        3
    );
}

#[tokio::test]
async fn file_inspection_detects_source_changes_and_read_failures() {
    let temp = tempfile::tempdir().expect("temp");
    let removed = temp.path().join("removed");
    std::fs::write(&removed, b"abc").expect("removed fixture");
    assert!(
        inspect_with_hook(&removed, |path| std::fs::remove_file(path).expect("remove"))
            .await
            .is_err()
    );

    let truncated = temp.path().join("truncated");
    std::fs::write(&truncated, b"abc").expect("truncated fixture");
    assert!(
        inspect_with_hook(&truncated, |path| {
            std::fs::write(path, b"").expect("truncate");
        })
        .await
        .is_err()
    );

    #[cfg(unix)]
    {
        let directory = temp.path().join("directory");
        std::fs::write(&directory, b"abc").expect("directory fixture");
        assert!(
            inspect_with_hook(&directory, |path| {
                std::fs::remove_file(path).expect("remove file");
                std::fs::create_dir(path).expect("replace directory");
            })
            .await
            .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn discovery_skips_symlinks_and_rejects_special_entries() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let temp = tempfile::tempdir().expect("temp");
    let file = temp.path().join("file");
    std::fs::write(&file, b"x").expect("file");
    let link = temp.path().join("link");
    symlink(&file, &link).expect("symlink");
    assert!(discover(&upload_args(link, false)).is_err());

    let tree = temp.path().join("tree");
    std::fs::create_dir(&tree).expect("tree");
    symlink(&file, tree.join("skipped")).expect("nested symlink");
    std::fs::write(tree.join("kept"), b"x").expect("kept");
    assert_eq!(
        discover(&upload_args(tree.clone(), false))
            .expect("tree")
            .len(),
        1
    );
    let socket_path = tree.join("socket");
    let _listener = UnixListener::bind(&socket_path).expect("socket");
    assert!(discover(&upload_args(tree, false)).is_err());

    let socket_source = temp.path().join("source-socket");
    let _source_listener = UnixListener::bind(&socket_source).expect("source socket");
    assert!(discover(&upload_args(socket_source, false)).is_err());

    let invalid_name = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff, 0xfe]));
    assert!(file_entry(&invalid_name, None).is_err());
    assert!(directory_name(&invalid_name).is_err());

    let locked = temp.path().join("locked");
    std::fs::create_dir(&locked).expect("locked");
    std::fs::write(locked.join("file"), b"x").expect("locked child");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("lock");
    assert!(discover(&upload_args(locked.clone(), false)).is_err());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("unlock");
}

fn upload_args(source: PathBuf, include_ignored: bool) -> UploadArgs {
    UploadArgs {
        source,
        path: None,
        include_ignored,
    }
}
