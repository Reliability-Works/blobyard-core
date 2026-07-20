use super::super::contracts;
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_contract::StoredObjectRecord;
use blobyard_core::Slug;

fn object(path: &str, id: &str, size: Option<u64>) -> StoredObjectRecord {
    let mut object = crate::test_support::stored_object();
    object.version.object_path = path.to_owned();
    object.version.id = id.to_owned();
    object.version.size = size;
    object
}

#[test]
fn yard_names_and_host_labels_match_the_public_contract() {
    for reserved in ["admin", "api", "app", "docs", "www"] {
        assert_eq!(
            error_status(contracts::validate_yard_name(
                &Slug::new(reserved).expect("reserved slug")
            )),
            StatusCode::BAD_REQUEST
        );
    }
    let name = Slug::new("documentation").expect("yard name");
    let workspace = Slug::new("fixture").expect("workspace");
    assert!(contracts::validate_yard_name(&name).is_ok());
    let stable = contracts::stable_host_label(&name, &workspace, "yard_fixture");
    let deployment = contracts::deployment_host_label(&name, &workspace, "yarddeploy_fixture");
    assert_ne!(stable, deployment);
    assert!(stable.starts_with("documentation-"));
    assert!(stable.ends_with("-fixture"));
    assert!(stable.len() <= 63);
    let long = Slug::new("a".repeat(63)).expect("long slug");
    assert!(contracts::stable_host_label(&long, &long, "yard").len() <= 63);
}

#[test]
fn public_host_and_url_contracts_require_the_exact_isolated_origin() {
    let label = "documentation-123456789-fixture";
    assert_eq!(
        contracts::web_yard_url("https://yards.example.com", label).expect("yard URL"),
        format!("https://{label}.yards.example.com")
    );
    assert_eq!(
        contracts::public_host_label(
            "https://yards.example.com",
            &format!("{label}.yards.example.com")
        ),
        Some(label.to_owned())
    );
    for authority in [
        "yards.example.com",
        "other.example.com",
        "INVALID.yards.example.com",
        "-bad.yards.example.com",
        "bad-.yards.example.com",
    ] {
        assert!(
            contracts::public_host_label("https://yards.example.com", authority).is_none(),
            "{authority}"
        );
    }
    assert_eq!(
        error_status(contracts::web_yard_url("bad\norigin", label)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(contracts::web_yard_url(
            "https://yards.example.com",
            "invalid host"
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(contracts::public_host_label("bad\norigin", "anything").is_none());
}

#[test]
fn public_paths_preserve_directories_and_reject_ambiguous_or_unsafe_input() {
    for (path, expected) in [
        ("/", ""),
        ("/docs/", "docs/"),
        ("/guide", "guide"),
        ("/hello%20world", "hello world"),
    ] {
        assert_eq!(
            contracts::public_request_path(path).expect("public path"),
            expected
        );
    }
    for path in [
        "relative",
        "/double//path",
        "/query?value",
        "/fragment#value",
        "/back\\slash",
        "/%FF",
        "/%2F",
        "/%5C",
        "/%00",
        "/./",
        "/../",
    ] {
        assert_eq!(
            error_status(contracts::public_request_path(path)),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[test]
fn manifest_snapshot_requires_one_complete_normalized_indexed_file_set() {
    let root = ".blobyard-yard/yard/deploy/";
    assert_eq!(
        error_status(contracts::snapshot_manifest(root, Vec::new())),
        StatusCode::BAD_REQUEST
    );
    for files in [
        vec![object(&format!("{root}page.html"), "page", Some(1))],
        vec![object("foreign/index.html", "foreign", Some(1))],
        vec![object(&format!("{root}../index.html"), "unsafe", Some(1))],
        vec![object(&format!("{root}index.html"), "incomplete", None)],
        vec![
            object(&format!("{root}index.html"), "one", Some(1)),
            object(&format!("{root}index.html"), "two", Some(1)),
        ],
        vec![
            object(&format!("{root}index.html"), "index", Some(u64::MAX)),
            object(&format!("{root}asset.js"), "asset", Some(1)),
        ],
    ] {
        assert_eq!(
            error_status(contracts::snapshot_manifest(root, files)),
            StatusCode::BAD_REQUEST
        );
    }
    let snapshot = contracts::snapshot_manifest(
        root,
        vec![
            object(&format!("{root}index.html"), "index", Some(5)),
            object(&format!("{root}asset.js"), "asset", Some(3)),
        ],
    )
    .expect("manifest snapshot");
    assert_eq!(snapshot.files[0].normalized_path, "asset.js");
    assert_eq!(snapshot.files[1].normalized_path, "index.html");
    assert_eq!(snapshot.total_bytes, 8);
}
