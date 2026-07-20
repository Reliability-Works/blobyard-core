use super::{InboxStatus, NewInbox};

#[test]
fn inbox_status_round_trips_and_rejects_unknown_values() {
    for status in [InboxStatus::Active, InboxStatus::Revoked] {
        assert_eq!(InboxStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(InboxStatus::parse("expired"), None);
}

#[test]
fn inbox_values_preserve_the_provider_independent_contract() {
    let inbox = NewInbox {
        id: "inbox_1".to_owned(),
        workspace_id: "workspace_1".to_owned(),
        project_id: "project_1".to_owned(),
        name: "Client logs".to_owned(),
        capability_hash: "a".repeat(64),
        expires_at_ms: 2,
        maximum_files: 20,
        maximum_bytes: 1024,
        created_at_ms: 1,
    };
    assert_eq!(inbox.name, "Client logs");
    assert_eq!(inbox.maximum_files, 20);
}
