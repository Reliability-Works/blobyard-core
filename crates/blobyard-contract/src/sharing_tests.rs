use super::ShareStatus;

#[test]
fn share_status_round_trips_every_persisted_value() {
    for status in [
        ShareStatus::Active,
        ShareStatus::Exhausted,
        ShareStatus::Revoked,
    ] {
        assert_eq!(ShareStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(ShareStatus::parse("expired"), None);
}
