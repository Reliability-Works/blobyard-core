#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;

fn graph() -> ApplicationPolicyGraph {
    ApplicationPolicyGraph {
        default_role: Some("viewer".to_owned()),
        roles: BTreeMap::from([
            (
                "editor".to_owned(),
                ApplicationRoleDefinition {
                    inherits: vec!["viewer".to_owned()],
                    permissions: vec!["items.write".to_owned()],
                },
            ),
            (
                "viewer".to_owned(),
                ApplicationRoleDefinition {
                    inherits: Vec::new(),
                    permissions: vec!["items.read".to_owned()],
                },
            ),
        ]),
    }
}

#[test]
fn canonicalizes_and_expands_role_graphs() {
    let canonical = canonicalize_application_policy(graph()).expect("valid graph");
    assert_eq!(
        canonical.effective.effective_roles["editor"],
        ["editor", "viewer"]
    );
    assert_eq!(
        canonical.effective.effective_permissions["editor"],
        ["items.read", "items.write"]
    );
    assert_eq!(
        canonical.effective.effective_permissions["viewer"],
        ["items.read"]
    );
}

#[test]
fn sorts_direct_arrays_before_persistence() {
    let mut input = graph();
    input.roles.insert(
        "owner".to_owned(),
        ApplicationRoleDefinition {
            inherits: vec!["viewer".to_owned(), "editor".to_owned()],
            permissions: vec!["z.read".to_owned(), "a.read".to_owned()],
        },
    );
    let canonical = canonicalize_application_policy(input).expect("valid graph");
    assert_eq!(
        canonical.graph.roles["owner"].inherits,
        ["editor", "viewer"]
    );
    assert_eq!(
        canonical.graph.roles["owner"].permissions,
        ["a.read", "z.read"]
    );
}

#[test]
fn rejects_invalid_graph_shapes() {
    let cases = [
        ApplicationPolicyGraph {
            default_role: Some("missing".to_owned()),
            ..graph()
        },
        ApplicationPolicyGraph {
            roles: BTreeMap::from([(
                "Bad".to_owned(),
                ApplicationRoleDefinition {
                    inherits: Vec::new(),
                    permissions: Vec::new(),
                },
            )]),
            default_role: None,
        },
        ApplicationPolicyGraph {
            roles: BTreeMap::from([(
                "viewer".to_owned(),
                ApplicationRoleDefinition {
                    inherits: vec!["missing".to_owned()],
                    permissions: Vec::new(),
                },
            )]),
            default_role: None,
        },
    ];
    for input in cases {
        assert_eq!(
            canonicalize_application_policy(input),
            Err(ApplicationPolicyError)
        );
    }
}

#[test]
fn rejects_cycles_duplicates_permissions_and_limits() {
    let mut cycle = graph();
    cycle.roles.get_mut("viewer").expect("viewer").inherits = vec!["editor".to_owned()];
    let mut duplicate = graph();
    duplicate
        .roles
        .get_mut("viewer")
        .expect("viewer")
        .permissions = vec!["items.read".to_owned(), "items.read".to_owned()];
    let mut bad_permission = graph();
    bad_permission
        .roles
        .get_mut("viewer")
        .expect("viewer")
        .permissions = vec!["invalid".to_owned()];
    let too_many = ApplicationPolicyGraph {
        default_role: None,
        roles: (0..=MAXIMUM_APPLICATION_POLICY_ROLES)
            .map(|index| {
                (
                    format!("role-{index}"),
                    ApplicationRoleDefinition {
                        inherits: Vec::new(),
                        permissions: Vec::new(),
                    },
                )
            })
            .collect(),
    };
    for input in [cycle, duplicate, bad_permission, too_many] {
        assert!(canonicalize_application_policy(input).is_err());
    }
}

#[test]
fn validates_manifest_digest_shape() {
    assert!(valid_source_manifest_digest(&"a".repeat(64)));
    for value in ["", &"a".repeat(63), &"A".repeat(64), &"g".repeat(64)] {
        assert!(!valid_source_manifest_digest(value));
    }
}
