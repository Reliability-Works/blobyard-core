//! Public CI action vocabulary conformance.

use blobyard_contract::CiAction;

#[test]
fn ci_actions_round_trip_exact_public_names() {
    for (action, name) in [
        (CiAction::Download, "download"),
        (CiAction::Share, "share"),
        (CiAction::Upload, "upload"),
        (CiAction::YardManage, "yard:manage"),
    ] {
        assert_eq!(action.as_str(), name);
        assert_eq!(CiAction::parse(name), Some(action));
    }
    assert_eq!(CiAction::parse("yard_manage"), None);
}
