use super::super::contracts;
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_contract::{PreviewRecord, PreviewStatus, StoredObjectRecord};
use blobyard_core::{SecretString, WebYardOrigin};

fn object(path: &str, id: &str) -> StoredObjectRecord {
    let mut object = crate::test_support::stored_object();
    object.version.id = id.to_owned();
    object.version.object_path = path.to_owned();
    object
}

#[test]
fn manifest_identifiers_accept_the_canonical_shape_and_reject_each_invalid_component() {
    assert_eq!(
        contracts::manifest_root("1234567890abcdef").expect("manifest root"),
        ".blobyard-preview/1234567890abcdef/"
    );
    for manifest_id in [
        "a".repeat(15),
        "a".repeat(129),
        format!("-{}", "a".repeat(15)),
        format!("{}!", "a".repeat(15)),
    ] {
        assert_eq!(
            error_status(contracts::manifest_root(&manifest_id)),
            StatusCode::BAD_REQUEST,
            "{manifest_id}"
        );
    }
}

#[test]
fn snapshot_manifest_rejects_empty_oversized_duplicate_and_incomplete_manifests() {
    let root = ".blobyard-preview/manifest/";
    assert_eq!(
        error_status(contracts::snapshot_manifest(root, Vec::new())),
        StatusCode::BAD_REQUEST
    );
    let oversized = (0..=10_000)
        .map(|index| object(&format!("{root}{index}.html"), &format!("version_{index}")))
        .collect();
    assert_eq!(
        error_status(contracts::snapshot_manifest(root, oversized)),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        error_status(contracts::snapshot_manifest(
            root,
            vec![object(&format!("{root}page.html"), "version_page")],
        )),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        error_status(contracts::snapshot_manifest(
            root,
            vec![
                object(&format!("{root}index.html"), "version_one"),
                object(&format!("{root}index.html"), "version_two"),
            ],
        )),
        StatusCode::CONFLICT
    );
}

#[test]
fn snapshot_manifest_rejects_foreign_and_unsafe_object_paths() {
    let root = ".blobyard-preview/manifest/";
    for path in [
        "other/index.html",
        ".blobyard-preview/manifest/../index.html",
    ] {
        assert_eq!(
            error_status(contracts::snapshot_manifest(
                root,
                vec![object(path, "version")],
            )),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn public_path_contract_normalizes_directories_and_rejects_ambiguous_input() {
    assert_eq!(
        contracts::public_preview_path("/docs/").expect("directory path"),
        "docs/index.html"
    );
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
            error_status(contracts::public_preview_path(path)),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    assert_eq!(
        error_status(contracts::public_preview_path(&format!(
            "/{}",
            "a".repeat(1_025)
        ))),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn public_host_and_url_contracts_require_the_exact_isolated_origin() {
    let capability = SecretString::new("a".repeat(52)).expect("capability");
    assert_eq!(
        contracts::preview_url("https://yards.example.com", &capability)
            .expect("preview URL")
            .expose_secret(),
        "https://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.yards.example.com/"
    );
    assert_eq!(
        error_status(contracts::preview_url("bad\norigin", &capability)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(contracts::preview_url(
            "https://example.com",
            &SecretString::new("invalid.label").expect("invalid DNS capability fixture"),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(contracts::public_host_capability("bad\norigin", "a.example.com").is_none());
    assert!(contracts::public_host_capability("https://example.com", "other.test").is_none());
    for capability in [
        "a".repeat(51),
        "A".repeat(52),
        format!("{}b", "a".repeat(51)),
    ] {
        assert!(
            contracts::public_host_capability(
                "https://example.com",
                &format!("{capability}.example.com"),
            )
            .is_none()
        );
    }
    assert!(WebYardOrigin::new("https://example.com").is_ok());
}

#[test]
fn preview_status_and_time_formatting_cover_every_persisted_state() {
    let record = |status, expires_at_ms| PreviewRecord {
        id: "preview".to_owned(),
        workspace_id: "workspace".to_owned(),
        project_id: "project".to_owned(),
        expires_at_ms,
        status,
        created_at_ms: 1,
        revoked_at_ms: None,
    };
    assert_eq!(
        contracts::status(&record(PreviewStatus::Revoked, 10), 1),
        "revoked"
    );
    assert_eq!(
        contracts::status(&record(PreviewStatus::Active, 10), 10),
        "expired"
    );
    assert_eq!(
        contracts::status(&record(PreviewStatus::Active, 10), 9),
        "active"
    );
    assert_eq!(
        error_status(contracts::formatted_time(u64::MAX)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
