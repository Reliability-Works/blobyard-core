//! Canonical Blobyard URI parsing and rejection contract tests.

#![allow(clippy::panic, reason = "parse setup failures must fail the test")]

use blobyard_core::{BlobyardUri, BlobyardUriError, Slug};
use std::num::NonZeroU64;

#[test]
fn parses_and_canonicalizes_a_versioned_uri() {
    let result =
        "blobyard://team-alpha/mobile/builds/My%20App.ipa?version=12".parse::<BlobyardUri>();
    let Ok(uri) = result else {
        panic!("valid URI should parse");
    };

    assert_eq!(uri.workspace(), "team-alpha");
    assert_eq!(uri.project(), "mobile");
    assert_eq!(uri.logical_path(), "builds/My App.ipa");
    assert_eq!(uri.version(), NonZeroU64::new(12));
    assert_eq!(
        uri.to_string(),
        "blobyard://team-alpha/mobile/builds/My%20App.ipa?version=12"
    );
}

#[test]
fn preserves_unicode_and_unversioned_paths() {
    let result = "blobyard://studio/default/screenshots/%E2%9C%93.png".parse::<BlobyardUri>();
    let Ok(uri) = result else {
        panic!("valid Unicode URI should parse");
    };

    assert_eq!(uri.logical_path(), "screenshots/✓.png");
    assert_eq!(uri.version(), None);
    assert_eq!(
        uri.to_string(),
        "blobyard://studio/default/screenshots/%E2%9C%93.png"
    );
}

#[test]
fn constructor_rejects_unsafe_or_expansively_encoded_paths() {
    let workspace = slug("studio");
    let project = slug("default");
    assert_eq!(
        BlobyardUri::new(workspace.clone(), project.clone(), "a//b".to_owned(), None),
        Err(BlobyardUriError::InvalidPath)
    );
    assert_eq!(
        BlobyardUri::new(workspace, project, "!".repeat(1_366), None),
        Err(BlobyardUriError::TooLong)
    );
    let workspace = slug("studio");
    let project = slug("default");
    assert_eq!(
        BlobyardUri::new(workspace, project, ".".to_owned(), None),
        Err(BlobyardUriError::InvalidPath)
    );
}

fn slug(value: &str) -> Slug {
    let Ok(slug) = Slug::new(value.to_owned()) else {
        panic!("valid fixture slug");
    };
    slug
}

#[test]
fn rejects_invalid_structure_and_slugs() {
    let oversized = format!("blobyard://studio/default/{}", "a".repeat(4_096));
    let cases = [
        (oversized.as_str(), BlobyardUriError::TooLong),
        (
            "https://studio/default/file",
            BlobyardUriError::InvalidScheme,
        ),
        ("blobyard://studio", BlobyardUriError::InvalidStructure),
        (
            "blobyard://studio/default",
            BlobyardUriError::InvalidStructure,
        ),
        (
            "blobyard:///default/file",
            BlobyardUriError::InvalidStructure,
        ),
        (
            "blobyard://-studio/default/file",
            BlobyardUriError::InvalidWorkspace,
        ),
        (
            "blobyard://studio/project./file",
            BlobyardUriError::InvalidProject,
        ),
        (
            "blobyard://studio/default/file?version=1?x",
            BlobyardUriError::InvalidStructure,
        ),
        (
            "blobyard://studio/default/file#fragment",
            BlobyardUriError::InvalidStructure,
        ),
    ];

    assert_rejected(&cases);
}

#[test]
fn rejects_unsafe_or_ambiguous_paths() {
    let long_path = format!("blobyard://studio/default/{}", "a".repeat(2_049));
    let cases = [
        (long_path.as_str(), BlobyardUriError::TooLong),
        (
            "blobyard://studio/default/../file",
            BlobyardUriError::InvalidPath,
        ),
        (
            "blobyard://studio/default/%2E%2E",
            BlobyardUriError::InvalidPath,
        ),
        (
            "blobyard://studio/default/a%2Fb",
            BlobyardUriError::InvalidPath,
        ),
        (
            "blobyard://studio/default/a\\b",
            BlobyardUriError::InvalidPath,
        ),
        (
            "blobyard://studio/default//file",
            BlobyardUriError::InvalidPath,
        ),
        (
            "blobyard://studio/default/%00",
            BlobyardUriError::InvalidPath,
        ),
    ];

    assert_rejected(&cases);
}

#[test]
fn rejects_invalid_encoding_and_versions() {
    let cases = [
        (
            "blobyard://studio/default/%",
            BlobyardUriError::InvalidPercentEncoding,
        ),
        (
            "blobyard://studio/default/%FF",
            BlobyardUriError::InvalidPercentEncoding,
        ),
        (
            "blobyard://studio/default/file?version=0",
            BlobyardUriError::InvalidVersion,
        ),
        (
            "blobyard://studio/default/file?version=x",
            BlobyardUriError::InvalidVersion,
        ),
        (
            "blobyard://studio/default/file?other=1",
            BlobyardUriError::InvalidVersion,
        ),
        (
            "blobyard://studio/default/file?version=1&x=2",
            BlobyardUriError::InvalidVersion,
        ),
    ];

    assert_rejected(&cases);
}

fn assert_rejected(cases: &[(&str, BlobyardUriError)]) {
    for (input, expected) in cases {
        assert_eq!(input.parse::<BlobyardUri>(), Err(*expected), "{input}");
    }
}

#[test]
fn uri_errors_have_actionable_messages() {
    let errors = [
        BlobyardUriError::TooLong,
        BlobyardUriError::InvalidScheme,
        BlobyardUriError::InvalidStructure,
        BlobyardUriError::InvalidWorkspace,
        BlobyardUriError::InvalidProject,
        BlobyardUriError::InvalidPath,
        BlobyardUriError::InvalidPercentEncoding,
        BlobyardUriError::InvalidVersion,
    ];

    for error in errors {
        assert!(!error.to_string().is_empty());
    }
}
