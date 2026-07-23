use super::{YardAccessPrincipalKind, YardVisibility};

#[test]
fn visibilities_round_trip_and_reject_unknown_values() {
    for visibility in [
        YardVisibility::Public,
        YardVisibility::Owner,
        YardVisibility::Selected,
        YardVisibility::Workspace,
        YardVisibility::AuthenticatedLink,
        YardVisibility::AnyAuthenticated,
    ] {
        assert_eq!(YardVisibility::parse(visibility.as_str()), Some(visibility));
    }
    assert_eq!(YardVisibility::parse("unknown"), None);
}

#[test]
fn principal_kinds_round_trip_and_reject_unknown_values() {
    for kind in [
        YardAccessPrincipalKind::User,
        YardAccessPrincipalKind::Group,
        YardAccessPrincipalKind::GuestInvite,
        YardAccessPrincipalKind::Link,
    ] {
        assert_eq!(YardAccessPrincipalKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(YardAccessPrincipalKind::parse("unknown"), None);
}
