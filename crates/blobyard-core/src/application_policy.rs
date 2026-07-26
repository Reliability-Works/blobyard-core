use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum number of declared application roles in one approved policy.
pub const MAXIMUM_APPLICATION_POLICY_ROLES: usize = 32;
/// Maximum number of direct inherited roles in one role definition.
pub const MAXIMUM_DIRECT_INHERITED_ROLES: usize = 8;
/// Maximum number of direct permissions in one role definition.
pub const MAXIMUM_DIRECT_ROLE_PERMISSIONS: usize = 64;

/// One owner-approved application-role definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationRoleDefinition {
    /// Directly inherited declared roles.
    pub inherits: Vec<String>,
    /// Direct permissions declared by this role.
    pub permissions: Vec<String>,
}

/// Canonical owner-approved Yard application-role graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationPolicyGraph {
    /// Role applied to every admitted authenticated private session.
    pub default_role: Option<String>,
    /// Declared role definitions keyed by stable role name.
    pub roles: BTreeMap<String, ApplicationRoleDefinition>,
}

/// Precomputed deterministic transitive closure for one application policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectiveApplicationPolicy {
    /// Each role plus every transitively inherited role.
    pub effective_roles: BTreeMap<String, Vec<String>>,
    /// Sorted permission union for each role and its inheritance closure.
    pub effective_permissions: BTreeMap<String, Vec<String>>,
}

/// Validated canonical graph and its deterministic precomputed closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalApplicationPolicy {
    /// Canonically ordered declared graph.
    pub graph: ApplicationPolicyGraph,
    /// Canonically ordered transitive closure.
    pub effective: EffectiveApplicationPolicy,
}

/// Stable application-policy validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationPolicyError;

/// Validates and canonicalizes an owner-approved application-role graph.
///
/// # Errors
///
/// Returns an error for invalid names, duplicates, undeclared inheritance, cycles, or exceeded
/// manifest limits.
pub fn canonicalize_application_policy(
    mut graph: ApplicationPolicyGraph,
) -> Result<CanonicalApplicationPolicy, ApplicationPolicyError> {
    validate_graph(&graph)?;
    canonicalize_direct_arrays(&mut graph);
    let effective = expand(&graph);
    Ok(CanonicalApplicationPolicy { graph, effective })
}

/// Returns whether a digest is exactly 64 lowercase hexadecimal characters.
#[must_use]
pub fn valid_source_manifest_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_graph(graph: &ApplicationPolicyGraph) -> Result<(), ApplicationPolicyError> {
    if graph.roles.len() > MAXIMUM_APPLICATION_POLICY_ROLES
        || graph
            .default_role
            .as_ref()
            .is_some_and(|role| !graph.roles.contains_key(role))
    {
        return Err(ApplicationPolicyError);
    }
    for (name, definition) in &graph.roles {
        if !crate::application_manifest::patterns::role_name(name)
            || definition.inherits.len() > MAXIMUM_DIRECT_INHERITED_ROLES
            || definition.permissions.len() > MAXIMUM_DIRECT_ROLE_PERMISSIONS
            || !unique(&definition.inherits)
            || !unique(&definition.permissions)
            || definition
                .inherits
                .iter()
                .any(|inherited| !graph.roles.contains_key(inherited))
            || definition
                .permissions
                .iter()
                .any(|permission| !crate::application_manifest::patterns::permission(permission))
            || has_cycle(name, graph)
        {
            return Err(ApplicationPolicyError);
        }
    }
    Ok(())
}

fn unique(values: &[String]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn has_cycle(origin: &str, graph: &ApplicationPolicyGraph) -> bool {
    let mut pending = graph.roles[origin].inherits.clone();
    let mut visited = BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if candidate == origin {
            return true;
        }
        if visited.insert(candidate.clone()) {
            pending.extend(graph.roles[&candidate].inherits.iter().cloned());
        }
    }
    false
}

fn canonicalize_direct_arrays(graph: &mut ApplicationPolicyGraph) {
    for definition in graph.roles.values_mut() {
        definition.inherits.sort();
        definition.permissions.sort();
    }
}

fn expand(graph: &ApplicationPolicyGraph) -> EffectiveApplicationPolicy {
    let mut effective_roles = BTreeMap::new();
    let mut effective_permissions = BTreeMap::new();
    for name in graph.roles.keys() {
        let roles = inherited_closure(name, graph);
        let permissions = roles
            .iter()
            .flat_map(|role| graph.roles[role].permissions.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        effective_roles.insert(name.clone(), roles);
        effective_permissions.insert(name.clone(), permissions);
    }
    EffectiveApplicationPolicy {
        effective_roles,
        effective_permissions,
    }
}

fn inherited_closure(name: &str, graph: &ApplicationPolicyGraph) -> Vec<String> {
    let mut roles = BTreeSet::from([name.to_owned()]);
    let mut pending = graph.roles[name].inherits.clone();
    while let Some(candidate) = pending.pop() {
        if roles.insert(candidate.clone()) {
            pending.extend(graph.roles[&candidate].inherits.iter().cloned());
        }
    }
    roles.into_iter().collect()
}

#[cfg(test)]
#[path = "application_policy_tests.rs"]
mod tests;
