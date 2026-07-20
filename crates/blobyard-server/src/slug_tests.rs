use super::from_name;

#[test]
fn normalizes_safe_names_and_rejects_unsupported_characters() {
    assert_eq!(
        from_name("Release Builds").map(|slug| slug.to_string()),
        Some("release-builds".to_owned())
    );
    assert_eq!(
        from_name("Release ---").map(|slug| slug.to_string()),
        Some("release".to_owned())
    );
    assert_eq!(
        from_name(&"A".repeat(64)).map(|slug| slug.to_string()),
        Some("a".repeat(63))
    );
    assert!(from_name("unsupported/").is_none());
    assert!(from_name("---").is_none());
}
