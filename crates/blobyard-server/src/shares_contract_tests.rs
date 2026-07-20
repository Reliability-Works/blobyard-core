#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::contracts::{
    content_type_class, notification_status, share_download_expiry, share_expiry, share_page_html,
    share_summary, share_url,
};
use blobyard_api_client::ShareNotificationStatus;
use blobyard_contract::{ShareRecord, ShareStatus, ShareTarget};
use blobyard_core::SecretString;

fn share_record(status: ShareStatus, version_id: Option<String>) -> ShareRecord {
    ShareRecord {
        id: "share".to_owned(),
        workspace_id: "workspace".to_owned(),
        version_id,
        expires_at_ms: 10,
        status,
        consumed_count: 0,
        maximum_downloads: None,
        created_at_ms: 1,
        revoked_at_ms: None,
    }
}

#[test]
fn share_expiry_accepts_supported_durations_and_rejects_unsafe_values() {
    let day = 24 * 60 * 60 * 1_000;
    assert_eq!(share_expiry(1, None).expect("default"), 1 + 7 * day);
    for (duration, expected) in [
        ("1d", day),
        ("2h", 7_200_000),
        ("3m", 180_000),
        ("4s", 4_000),
    ] {
        assert_eq!(
            share_expiry(1, Some(duration)).expect("duration"),
            1 + expected
        );
    }
    for invalid in [
        "",
        "0s",
        "01s",
        "1w",
        "x1s",
        "31d",
        "18446744073709551615d",
        "18446744073709551616d",
    ] {
        assert!(share_expiry(1, Some(invalid)).is_err(), "{invalid}");
    }
    assert!(share_expiry(u64::MAX, None).is_err());
}

#[test]
fn share_notification_status_validates_email_without_claiming_delivery() {
    assert_eq!(
        notification_status(None).expect("none"),
        ShareNotificationStatus::NotRequested
    );
    assert_eq!(
        notification_status(Some("USER@example.com")).expect("email"),
        ShareNotificationStatus::Failed
    );
    for invalid in [
        "missing",
        "@example.com",
        "user@example",
        "user @example.com",
        "user\n@example.com",
    ] {
        assert!(notification_status(Some(invalid)).is_err(), "{invalid:?}");
    }
    let oversized = format!("{}@example.com", "a".repeat(243));
    assert!(notification_status(Some(&oversized)).is_err());
}

#[test]
fn share_urls_and_download_expiry_fail_closed() {
    let capability = SecretString::new("bysh_fixture").expect("capability");
    assert_eq!(
        share_url("https://example.com", &capability)
            .expect("share URL")
            .expose_secret(),
        "https://example.com/s/bysh_fixture"
    );
    assert!(share_url("bad\norigin", &capability).is_err());
    assert_eq!(share_download_expiry(100, 60, 120).expect("share cap"), 120);
    assert_eq!(
        share_download_expiry(100, 10, 120).expect("download cap"),
        110
    );
    assert!(share_download_expiry(u64::MAX, 1, u64::MAX).is_err());
}

#[test]
fn share_content_and_record_statuses_cover_every_contract_class() {
    for (content_type, class) in [
        ("audio/mpeg", "audio"),
        ("image/png", "image"),
        ("text/plain", "text"),
        ("video/mp4", "video"),
        ("application/json", "document"),
        ("application/problem+json", "document"),
        ("application/pdf", "document"),
        ("application/gzip", "archive"),
        ("application/x-7z-compressed", "archive"),
        ("application/x-tar", "archive"),
        ("application/zip", "archive"),
        ("application/octet-stream", "binary"),
        ("unknown", "binary"),
    ] {
        assert_eq!(content_type_class(content_type), class);
    }
    for (status, now, version, expected) in [
        (ShareStatus::Active, 2, Some("version"), "active"),
        (ShareStatus::Exhausted, 2, Some("version"), "exhausted"),
        (ShareStatus::Active, 10, Some("version"), "expired"),
        (ShareStatus::Active, 2, None, "unavailable"),
        (ShareStatus::Revoked, 2, Some("version"), "revoked"),
    ] {
        let summary =
            share_summary(share_record(status, version.map(str::to_owned)), now).expect("summary");
        assert_eq!(summary.status, expected);
    }
    let mut invalid = share_record(ShareStatus::Active, Some("version".to_owned()));
    invalid.expires_at_ms = u64::MAX;
    assert!(share_summary(invalid, 2).is_err());
}

#[test]
fn share_page_escapes_content_and_represents_download_availability() {
    let mut object = crate::test_support::stored_object();
    object.filename = "<&>'\"".to_owned();
    let active = ShareTarget {
        share: share_record(ShareStatus::Active, Some(object.version.id.clone())),
        object: object.clone(),
    };
    let page = share_page_html(&active, "/s/<&>'\"/download").expect("share page");
    assert!(page.contains("&lt;&amp;&gt;&#39;&quot;"));
    assert!(page.contains("/s/&lt;&amp;&gt;&#39;&quot;/download"));
    assert!(page.contains("<button type=\"submit\">Download</button>"));

    let exhausted = ShareTarget {
        share: share_record(ShareStatus::Exhausted, Some(object.version.id.clone())),
        object: object.clone(),
    };
    assert!(
        share_page_html(&exhausted, "/ignored")
            .expect("exhausted page")
            .contains("Download limit reached")
    );
    object.version.size = None;
    assert!(
        share_page_html(
            &ShareTarget {
                share: share_record(ShareStatus::Active, Some(object.version.id.clone())),
                object,
            },
            "/ignored",
        )
        .is_err()
    );

    let mut invalid_expiry = active;
    invalid_expiry.share.expires_at_ms = u64::MAX;
    assert!(share_page_html(&invalid_expiry, "/ignored").is_err());
}
