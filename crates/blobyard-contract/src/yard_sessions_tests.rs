use super::{YardSessionRecord, YardSessionStatus};

fn session(expires_at_ms: u64, revoked_at_ms: Option<u64>) -> YardSessionRecord {
    YardSessionRecord {
        id: "yardsession_fixture".to_owned(),
        token_hash: "ab".repeat(32),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "yardenv_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        created_at_ms: 1,
        expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms,
    }
}

#[test]
fn session_status_prefers_revocation_then_expiry() {
    assert_eq!(session(10, None).status_at(9), YardSessionStatus::Active);
    assert_eq!(session(10, None).status_at(10), YardSessionStatus::Expired);
    assert_eq!(
        session(10, Some(5)).status_at(10),
        YardSessionStatus::Revoked
    );
    assert_eq!(YardSessionStatus::Active.as_str(), "active");
    assert_eq!(YardSessionStatus::Expired.as_str(), "expired");
    assert_eq!(YardSessionStatus::Revoked.as_str(), "revoked");
}
