#![allow(
    clippy::redundant_pub_crate,
    reason = "the private sibling catalog module owns these shared schema helpers"
)]

use serde_json::{Map, Value, json};

pub(crate) fn tool_schema(
    name: &str,
    description: &str,
    properties: &Map<String, Value>,
    required: &[&str],
    annotations: &Value,
) -> Value {
    json!({
        "name": format!("blobyard_{name}"),
        "title": title(name),
        "description": description,
        "inputSchema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        },
        "outputSchema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "data": {} },
            "required": ["data"],
            "additionalProperties": false
        },
        "annotations": annotations
    })
}

pub(crate) fn scope_properties() -> Map<String, Value> {
    let mut properties = Map::new();
    add(
        &mut properties,
        "workspace",
        string("Optional workspace slug override."),
    );
    add(
        &mut properties,
        "project",
        string("Optional project slug override."),
    );
    properties
}

pub(crate) fn title(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn delete_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "uri", string("Blobyard URI to delete."));
    (
        "Delete a logical object and its retained versions.",
        vec!["uri"],
    )
}

pub(crate) fn revoke_share_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "share_id", string("Stable share identifier."));
    ("Revoke a public share link.", vec!["share_id"])
}

pub(crate) fn preview_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "directory",
        string("Local static directory containing index.html."),
    );
    add(
        properties,
        "expires",
        string("Optional lifetime such as 7d."),
    );
    (
        "Publish an isolated static preview and return its URL once.",
        vec!["directory"],
    )
}

pub(crate) fn upload_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "source",
        string("Local file or directory path."),
    );
    add(
        properties,
        "path",
        string("Optional destination logical path."),
    );
    add(
        properties,
        "include_ignored",
        boolean("Include files excluded by ignore rules."),
    );
    (
        "Upload a local file or directory through Blobyard's authorized transfer flow.",
        vec!["source"],
    )
}

pub(crate) fn download_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "uri", string("Blobyard URI to download."));
    add(properties, "output", string("Local destination path."));
    add(
        properties,
        "force",
        boolean("Replace an existing destination file."),
    );
    (
        "Download an object through Blobyard's authorized transfer flow.",
        vec!["uri", "output"],
    )
}

pub(crate) fn share_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "target",
        string("Local path or Blobyard URI to share."),
    );
    add(
        properties,
        "expires",
        string("Optional lifetime such as 7d."),
    );
    add(
        properties,
        "notify",
        string("Optional recipient email address."),
    );
    (
        "Create an expiring share and return its public URL once.",
        vec!["target"],
    )
}

pub(crate) fn inbox_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "name", string("Human-readable inbox name."));
    add(
        properties,
        "expires",
        string("Optional lifetime such as 24h."),
    );
    (
        "Create a guest upload inbox and return its public URL once.",
        vec!["name"],
    )
}

pub(crate) fn retention_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "latest",
        json!({
            "type": "integer", "minimum": 1, "maximum": u32::MAX,
            "description": "Number of newest matching versions to retain."
        }),
    );
    add(
        properties,
        "branch",
        string("Optional git branch provenance glob."),
    );
    add(properties, "path", string("Optional logical path glob."));
    (
        "Set or replace the selected project's retention policy.",
        vec!["latest"],
    )
}

pub(crate) fn deploy_yard_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "directory",
        string("Local static directory containing index.html."),
    );
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "spa",
        boolean("Use SPA fallback for unmatched extensionless paths."),
    );
    add(
        properties,
        "clean_urls",
        boolean("Resolve extensionless paths to matching HTML files."),
    );
    add(
        properties,
        "public",
        boolean("Must be true to acknowledge that deployed files are public."),
    );
    (
        "Deploy a static directory to a public Web Yard.",
        vec!["directory", "yard", "public"],
    )
}

pub(crate) fn list_yard_deploys_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    (
        "List immutable deploy history for a Web Yard.",
        vec!["yard"],
    )
}

pub(crate) fn rollback_yard_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "deploy_id",
        string("Optional immutable deploy identifier."),
    );
    ("Repoint a Web Yard to an earlier deploy.", vec!["yard"])
}

pub(crate) fn delete_yard_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "yard", string("Project-unique Web Yard name."));
    add(
        properties,
        "confirm",
        boolean("Must be true to confirm permanent Yard deletion."),
    );
    (
        "Delete a Web Yard and all of its deploys.",
        vec!["yard", "confirm"],
    )
}

pub(crate) fn add(properties: &mut Map<String, Value>, name: &str, schema: Value) {
    properties.insert(name.to_owned(), schema);
}

pub(crate) fn string(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "description": description })
}

pub(crate) fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}
