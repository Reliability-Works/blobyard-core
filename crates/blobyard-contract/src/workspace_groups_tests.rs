use super::{WorkspaceGroupStatus, normalize_group_name};
use crate::RepositoryError;

#[test]
fn statuses_round_trip_and_reject_unknown_values() {
    for status in [
        WorkspaceGroupStatus::Active,
        WorkspaceGroupStatus::Deactivated,
    ] {
        assert_eq!(WorkspaceGroupStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(WorkspaceGroupStatus::parse("unknown"), None);
}

#[test]
fn group_names_normalize_nfc_trim_unicode_whitespace_and_enforce_scalar_bounds() {
    assert_eq!(
        normalize_group_name("\u{2003}e\u{301}quipe\u{2003}"),
        Ok("équipe".to_owned())
    );
    assert_eq!(
        normalize_group_name("\u{2003}Team\u{2003}"),
        Ok("Team".to_owned())
    );
    assert_eq!(
        normalize_group_name("\u{feff}Team"),
        Ok("\u{feff}Team".to_owned())
    );
    for invalid in ["x", "x\u{7f}", "x\u{85}", "x\n"] {
        assert_eq!(
            normalize_group_name(invalid),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        normalize_group_name(&"x".repeat(81)),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(normalize_group_name("🚀🚀"), Ok("🚀🚀".to_owned()));
}
