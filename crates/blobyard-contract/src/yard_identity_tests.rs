use super::*;

#[test]
fn management_roles_have_stable_ordered_representations() {
    let roles = [
        (YardManagementRole::Owner, 0),
        (YardManagementRole::Admin, 1),
        (YardManagementRole::Developer, 2),
        (YardManagementRole::Auditor, 3),
    ];
    for (role, precedence) in roles {
        assert_eq!(role.precedence(), precedence);
        assert_eq!(YardManagementRole::parse(role.as_str()), Some(role));
    }
    assert_eq!(YardManagementRole::parse("workspace-owner"), None);
}
