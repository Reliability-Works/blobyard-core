use super::PreviewStatus;

#[test]
fn preview_status_round_trips_every_persisted_value() {
    for status in [PreviewStatus::Active, PreviewStatus::Revoked] {
        assert_eq!(PreviewStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(PreviewStatus::parse("expired"), None);
}
