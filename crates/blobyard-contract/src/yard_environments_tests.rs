use super::{YardEnvironmentKind, YardEnvironmentStatus};

#[test]
fn environment_kinds_round_trip_and_reject_unknown_values() {
    for kind in [
        YardEnvironmentKind::Production,
        YardEnvironmentKind::Staging,
        YardEnvironmentKind::Preview,
    ] {
        assert_eq!(YardEnvironmentKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(YardEnvironmentKind::parse("unknown"), None);
}

#[test]
fn environment_statuses_round_trip_and_reject_unknown_values() {
    for status in [
        YardEnvironmentStatus::Active,
        YardEnvironmentStatus::Deleted,
    ] {
        assert_eq!(YardEnvironmentStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(YardEnvironmentStatus::parse("unknown"), None);
}
