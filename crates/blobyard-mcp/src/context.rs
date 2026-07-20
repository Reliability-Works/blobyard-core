use crate::{Scope, ToolCall};
use serde_json::{Value, json};

pub(super) fn resource_call(uri: &str) -> Option<ToolCall> {
    if uri == "blobyard://session/identity" {
        return Some(ToolCall::Whoami {
            scope: Scope::default(),
        });
    }
    let path = uri.strip_prefix("blobyard://projects/")?;
    let (project, resource) = path.split_once('/')?;
    if project.is_empty() {
        return None;
    }
    let scope = Scope {
        workspace: None,
        project: Some(project.to_owned()),
    };
    match resource {
        "objects" => Some(ToolCall::ListObjects {
            scope,
            prefix: None,
            versions: false,
        }),
        "retention" => Some(ToolCall::GetRetention { scope }),
        _ => None,
    }
}

pub(super) fn prompts() -> Value {
    json!({ "prompts": [
        {
            "name": "blobyard_get_started",
            "title": "Get started with Blobyard",
            "description": "Use Blobyard safely with durable storage and expiring access.",
            "arguments": []
        },
        {
            "name": "artifact_handoff",
            "title": "Artifact handoff",
            "description": "Plan a secure Blobyard upload and expiring share handoff.",
            "arguments": [{
                "name": "project",
                "description": "Optional project slug to use.",
                "required": false
            }]
        }
    ] })
}

pub(super) fn prompt(name: &str, project: Option<&str>) -> Option<Value> {
    if name == "blobyard_get_started" {
        return Some(get_started());
    }
    (name == "artifact_handoff").then(|| artifact_handoff(project))
}

fn get_started() -> Value {
    json!({
        "description": "Safe first steps for Blobyard.",
        "messages": [{
            "role": "user",
            "content": {
                "type": "text",
                "text": "Start with blobyard_whoami and blobyard_list_projects to confirm identity and scope. Stored objects are durable by default. Share links, inbox capabilities, and transfer grants may expire, but their expiry does not delete stored objects. Ask for confirmation before creating any public capability or changing retention. Return a public capability URL only after its explicit creation tool succeeds. Never expose its token independently, or place credentials, authorization headers, cookies, OTPs, provider secrets, or signed storage URLs in model context."
            }
        }]
    })
}

fn artifact_handoff(project: Option<&str>) -> Value {
    let scope = project.map_or_else(
        || "Confirm the intended workspace and project before uploading.".to_owned(),
        |slug| format!("Use the Blobyard project `{slug}`."),
    );
    json!({
        "description": "A safe artifact upload and sharing workflow.",
        "messages": [{
            "role": "user",
            "content": {
                "type": "text",
                "text": format!(
                    "{scope} Inspect identity and scope, upload the requested artifact, then create an expiring share only after confirming the target and lifetime. Return the public share URL once after creation, but never expose its token independently or place credentials, authorization headers, cookies, OTPs, provider secrets, or signed storage URLs in model context."
                )
            }
        }]
    })
}

pub(super) fn resources() -> Value {
    json!({ "resources": [{
        "uri": "blobyard://session/identity",
        "name": "blobyard_identity",
        "title": "Blobyard identity metadata",
        "description": "Metadata for the authenticated identity and selected scope. Read it with the blobyard_whoami tool.",
        "mimeType": "application/json",
        "annotations": { "audience": ["assistant"], "priority": 0.8 }
    }] })
}

pub(super) fn resource_templates() -> Value {
    json!({ "resourceTemplates": [
        {
            "uriTemplate": "blobyard://projects/{project}/objects",
            "name": "blobyard_project_objects",
            "title": "Blobyard project object metadata",
            "description": "Redacted object metadata for a project. Retrieve it with blobyard_list_objects.",
            "mimeType": "application/json",
            "annotations": { "audience": ["assistant"], "priority": 0.9 }
        },
        {
            "uriTemplate": "blobyard://projects/{project}/retention",
            "name": "blobyard_project_retention",
            "title": "Blobyard project retention metadata",
            "description": "Retention policy metadata for a project. Retrieve it with blobyard_get_retention.",
            "mimeType": "application/json",
            "annotations": { "audience": ["assistant"], "priority": 0.7 }
        }
    ] })
}
