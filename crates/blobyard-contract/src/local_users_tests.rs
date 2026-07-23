use super::LocalUserStatus;

#[test]
fn user_statuses_round_trip_and_reject_unknown_values() {
    for status in [LocalUserStatus::Active, LocalUserStatus::Deactivated] {
        assert_eq!(LocalUserStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(LocalUserStatus::parse("unknown"), None);
}
