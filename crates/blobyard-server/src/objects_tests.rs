#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{object_source, object_summary, validate_prefix};
use blobyard_core::Slug;

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("fixture slug")
}

#[test]
fn object_summary_requires_complete_valid_version_metadata() {
    let summary = object_summary(
        &slug("workspace"),
        &slug("project"),
        crate::test_support::stored_object(),
    )
    .expect("valid object summary");
    assert_eq!(summary.size_bytes, 42);
    assert_eq!(summary.source, blobyard_api_client::ObjectSource::Cli);
    assert_eq!(
        summary.uri.to_string(),
        "blobyard://workspace/project/builds/app.zip?version=1"
    );

    let mut invalid = crate::test_support::stored_object();
    invalid.version.version = 0;
    assert!(object_summary(&slug("workspace"), &slug("project"), invalid).is_err());

    let mut invalid = crate::test_support::stored_object();
    invalid.version.object_path = "/absolute".to_owned();
    assert!(object_summary(&slug("workspace"), &slug("project"), invalid).is_err());

    let mut invalid = crate::test_support::stored_object();
    invalid.version.size = None;
    assert!(object_summary(&slug("workspace"), &slug("project"), invalid).is_err());

    let mut invalid = crate::test_support::stored_object();
    invalid.version.created_at_ms = u64::MAX;
    assert!(object_summary(&slug("workspace"), &slug("project"), invalid).is_err());
}

#[test]
fn object_prefixes_are_relative_bounded_and_segment_safe() {
    for prefix in [None, Some("builds"), Some("builds/releases/")] {
        assert!(validate_prefix(prefix).is_ok(), "{prefix:?}");
    }
    let overlong = "x".repeat(513);
    for prefix in [
        "",
        overlong.as_str(),
        "/absolute",
        "windows\\path",
        "line\nbreak",
        "builds//app",
        "builds/./app",
        "builds/../app",
    ] {
        assert!(validate_prefix(Some(prefix)).is_err(), "{prefix:?}");
    }
}

#[test]
fn every_persisted_object_source_maps_to_the_public_contract() {
    for (stored, public) in [
        (
            blobyard_contract::ObjectSource::Ci,
            blobyard_api_client::ObjectSource::Ci,
        ),
        (
            blobyard_contract::ObjectSource::Cli,
            blobyard_api_client::ObjectSource::Cli,
        ),
        (
            blobyard_contract::ObjectSource::Inbox,
            blobyard_api_client::ObjectSource::Inbox,
        ),
        (
            blobyard_contract::ObjectSource::Preview,
            blobyard_api_client::ObjectSource::Preview,
        ),
        (
            blobyard_contract::ObjectSource::Web,
            blobyard_api_client::ObjectSource::Web,
        ),
    ] {
        assert_eq!(object_source(stored), public);
    }
}
