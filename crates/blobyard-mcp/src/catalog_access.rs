#![allow(
    clippy::redundant_pub_crate,
    reason = "the private sibling catalog module owns these access schema helpers"
)]

use crate::catalog_contracts::{add, string};
use serde_json::{Map, Value, json};

pub(crate) fn yard_access_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    (
        "Show a Web Yard's effective visibility and active access grants.",
        vec!["yard"],
    )
}

pub(crate) fn set_yard_visibility_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "visibility",
        string(
            "Audience: public, owner, selected, workspace, authenticated-link, or any-authenticated.",
        ),
    );
    (
        "Set a Web Yard's visibility policy.",
        vec!["yard", "visibility"],
    )
}

pub(crate) fn grant_yard_access_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "principal_kind",
        string("Principal kind: user, group, guest-invite, or link."),
    );
    add(
        properties,
        "principal_id",
        string("Stable principal identifier."),
    );
    add(
        properties,
        "roles",
        string_array("Application roles granted to the principal."),
    );
    add(
        properties,
        "environment_id",
        string("Optional environment identifier restriction."),
    );
    add(
        properties,
        "expires_at",
        string("Optional RFC 3339 expiry timestamp."),
    );
    (
        "Grant one principal scoped access to a Web Yard.",
        vec!["yard", "principal_kind", "principal_id"],
    )
}

pub(crate) fn revoke_yard_access_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(properties, "grant_id", string("Stable grant identifier."));
    (
        "Revoke one Web Yard access grant.",
        vec!["yard", "grant_id"],
    )
}

pub(crate) fn set_yard_access_roles_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(properties, "grant_id", string("Stable grant identifier."));
    add(
        properties,
        "roles",
        string_array("Replacement application roles; an empty array clears roles."),
    );
    (
        "Replace one active Yard access grant's application roles.",
        vec!["yard", "grant_id", "roles"],
    )
}

pub(crate) fn list_yard_management_roles_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "cursor",
        string("Optional opaque continuation cursor."),
    );
    ("List Yard management-role assignments.", vec!["yard"])
}

pub(crate) fn set_yard_management_role_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "user_id",
        string("Stable active local-user identifier."),
    );
    add(
        properties,
        "role",
        string("Management role: owner, admin, developer, or auditor."),
    );
    (
        "Create or change one Yard management-role assignment.",
        vec!["yard", "user_id", "role"],
    )
}

pub(crate) fn revoke_yard_management_role_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "user_id",
        string("Stable active local-user identifier."),
    );
    (
        "Revoke one Yard management-role assignment.",
        vec!["yard", "user_id"],
    )
}

pub(crate) fn get_yard_application_policy_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    (
        "Read the current approved Yard application policy.",
        vec!["yard"],
    )
}

pub(crate) fn set_yard_application_policy_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "source_manifest_digest",
        string("SHA-256 digest of the canonical source manifest."),
    );
    add(
        properties,
        "default_role",
        json!({
            "type": ["string", "null"],
            "description": "Optional declared default application role."
        }),
    );
    add(
        properties,
        "roles",
        json!({
            "type": "object",
            "description": "Role-definition map keyed by application role name.",
            "additionalProperties": {
                "type": "object",
                "properties": {
                    "inherits": string_array("Direct inherited roles."),
                    "permissions": string_array("Direct application permissions.")
                },
                "required": ["inherits", "permissions"],
                "additionalProperties": false
            }
        }),
    );
    (
        "Approve and activate one canonical Yard application policy.",
        vec!["yard", "source_manifest_digest", "default_role", "roles"],
    )
}

pub(crate) fn list_yard_sessions_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    (
        "List retained browser sessions for a Web Yard.",
        vec!["yard"],
    )
}

pub(crate) fn revoke_yard_session_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "session_id",
        string("Stable Yard browser-session identifier."),
    );
    (
        "Revoke one retained Web Yard browser session.",
        vec!["yard", "session_id"],
    )
}

fn string_array(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "description": description
    })
}
