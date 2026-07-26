use super::{ApplicationManifest, ManifestError, ManifestRole};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    let declared = manifest.auth.as_ref().and_then(|auth| auth.roles.as_ref());
    let empty = BTreeMap::new();
    let roles = declared.unwrap_or(&empty);
    if let Some(default) = manifest
        .auth
        .as_ref()
        .and_then(|auth| auth.default_role.as_ref())
    {
        reference(
            roles.contains_key(default),
            "auth.default_role",
            default,
            errors,
        );
    }
    for (name, role) in roles {
        inherits(name, role, roles, errors);
        if has_cycle(name, name, roles, &mut BTreeSet::new()) {
            errors.push(ManifestError::new(
                format!("auth.roles.{name}.inherits"),
                "inheritance must be acyclic",
            ));
        }
    }
    function_permissions(manifest, roles, errors);
}

fn inherits(
    name: &str,
    role: &ManifestRole,
    roles: &BTreeMap<String, ManifestRole>,
    errors: &mut Vec<ManifestError>,
) {
    for (index, inherited) in role
        .inherits
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        reference(
            roles.contains_key(inherited),
            &format!("auth.roles.{name}.inherits[{index}]"),
            inherited,
            errors,
        );
    }
}

fn has_cycle(
    origin: &str,
    current: &str,
    roles: &BTreeMap<String, ManifestRole>,
    visited: &mut BTreeSet<String>,
) -> bool {
    let Some(role) = roles.get(current) else {
        return false;
    };
    role.inherits
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|parent| {
            parent == origin
                || (visited.insert(parent.clone()) && has_cycle(origin, parent, roles, visited))
        })
}

fn function_permissions(
    manifest: &ApplicationManifest,
    roles: &BTreeMap<String, ManifestRole>,
    errors: &mut Vec<ManifestError>,
) {
    let declared = roles
        .values()
        .filter_map(|role| role.permissions.as_deref())
        .flatten()
        .collect::<BTreeSet<_>>();
    for (function_index, function) in manifest
        .functions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        for (permission_index, permission) in function
            .permissions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            if !declared.contains(permission) {
                errors.push(ManifestError::new(
                    format!("functions[{function_index}].permissions[{permission_index}]"),
                    format!("permission `{permission}` is not declared by any auth role"),
                ));
            }
        }
    }
}

fn reference(declared: bool, path: &str, name: &str, errors: &mut Vec<ManifestError>) {
    if !declared {
        errors.push(ManifestError::new(
            path,
            format!("references undeclared role `{name}`"),
        ));
    }
}
