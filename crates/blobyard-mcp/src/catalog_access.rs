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

fn string_array(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "description": description
    })
}
