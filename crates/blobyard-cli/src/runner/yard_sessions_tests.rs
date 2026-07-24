#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{session_lines, status_label};
use blobyard_api_client::{YardSessionStatus, YardSessionSummary};

fn session(status: YardSessionStatus, last_used_at: Option<&str>) -> YardSessionSummary {
    YardSessionSummary {
        created_at: "2026-07-24T09:00:00Z".to_owned(),
        environment_id: "yardenv_docs".to_owned(),
        expires_at: "2026-08-23T09:00:00Z".to_owned(),
        host_label: "docs-123456789-team".to_owned(),
        id: "byys_session".to_owned(),
        last_used_at: last_used_at.map(str::to_owned),
        status,
        user_display_name: "Avery Reader".to_owned(),
        user_id: "user_reader".to_owned(),
        yard_id: "yard_docs".to_owned(),
    }
}

#[test]
fn session_lines_cover_empty_and_populated_lists() {
    assert_eq!(session_lines(&[]), "No Yard browser sessions found.");
    assert_eq!(
        session_lines(&[
            session(YardSessionStatus::Active, Some("2026-07-24T10:00:00Z")),
            session(YardSessionStatus::Expired, None),
        ]),
        "byys_session\tactive\tAvery Reader\tdocs-123456789-team\tcreated \
         2026-07-24T09:00:00Z\texpires 2026-08-23T09:00:00Z\tlast used \
         2026-07-24T10:00:00Z\nbyys_session\texpired\tAvery Reader\t\
         docs-123456789-team\tcreated 2026-07-24T09:00:00Z\texpires \
         2026-08-23T09:00:00Z\tlast used never"
    );
}

#[test]
fn status_labels_cover_the_public_lifecycle() {
    assert_eq!(status_label(YardSessionStatus::Active), "active");
    assert_eq!(status_label(YardSessionStatus::Expired), "expired");
    assert_eq!(status_label(YardSessionStatus::Revoked), "revoked");
}
