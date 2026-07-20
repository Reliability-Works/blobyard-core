use super::{WebYardStatus, YardDeployStatus, is_valid_yard_path, is_valid_yard_request_path};

#[test]
fn yard_statuses_round_trip_and_reject_unknown_values() {
    for status in [
        WebYardStatus::Active,
        WebYardStatus::Suspended,
        WebYardStatus::Deleted,
    ] {
        assert_eq!(WebYardStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(WebYardStatus::parse("unknown"), None);
}

#[test]
fn deploy_statuses_round_trip_and_reject_unknown_values() {
    for status in [
        YardDeployStatus::Uploading,
        YardDeployStatus::Finalising,
        YardDeployStatus::Live,
        YardDeployStatus::Failed,
        YardDeployStatus::Superseded,
        YardDeployStatus::Pruned,
    ] {
        assert_eq!(YardDeployStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(YardDeployStatus::parse("unknown"), None);
}

#[test]
fn yard_paths_are_portable_normalized_relative_paths() {
    assert!(is_valid_yard_path("index.html"));
    assert!(is_valid_yard_path("assets/app.js"));
    assert!(!is_valid_yard_path(""));
    assert!(!is_valid_yard_path("/index.html"));
    assert!(!is_valid_yard_path("assets/../index.html"));
    assert!(!is_valid_yard_path("assets\\app.js"));
    assert!(!is_valid_yard_path(&"a".repeat(1_025)));

    assert!(is_valid_yard_request_path(""));
    assert!(is_valid_yard_request_path("docs/"));
    assert!(!is_valid_yard_request_path("/docs"));
    assert!(!is_valid_yard_request_path("/"));
}
