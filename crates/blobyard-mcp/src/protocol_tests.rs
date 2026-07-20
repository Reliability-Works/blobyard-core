#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "protocol tests use fixed JSON fixtures"
)]

use crate::{BackendError, BackendFuture, McpServer, ToolBackend, ToolCall, serve};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Mutex;
use tokio::io::BufReader;

#[path = "protocol_admin_tests.rs"]
mod admin_tests;
#[path = "protocol_error_tests.rs"]
mod error_tests;
#[path = "protocol_result_tests.rs"]
mod result_tests;
#[path = "transport_failure_tests.rs"]
mod transport_failure_tests;
#[path = "protocol_yard_tests.rs"]
mod yard_tests;

struct Backend {
    calls: Mutex<Vec<ToolCall>>,
    result: Mutex<Result<Value, BackendError>>,
}

impl Backend {
    fn success(value: Value) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            result: Mutex::new(Ok(value)),
        }
    }

    fn failure(error: BackendError) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            result: Mutex::new(Err(error)),
        }
    }
}

impl ToolBackend for Backend {
    fn call(&self, call: ToolCall) -> BackendFuture<'_> {
        self.calls
            .lock()
            .expect("call log lock must be available")
            .push(call);
        let result = self
            .result
            .lock()
            .expect("backend result lock must be available")
            .clone();
        Box::pin(async move { result })
    }
}

fn request(id: i32, method: &str, params: impl Serialize) -> String {
    let params = serde_json::to_value(params).expect("request parameters must serialize");
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

async fn initialize(server: &mut McpServer<'_, Backend>) -> Value {
    server
        .process_line(&request(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-11-25" }),
        ))
        .await
        .expect("initialize request must receive a response")
}

async fn initialized_server(backend: &Backend) -> McpServer<'_, Backend> {
    let mut server = McpServer::new(backend);
    initialize(&mut server).await;
    server
}

async fn call_whoami(backend: &Backend, expectation: &str) -> Value {
    call_tool(backend, "blobyard_whoami", json!({}), expectation).await
}

async fn call_tool(backend: &Backend, name: &str, arguments: Value, expectation: &str) -> Value {
    let mut server = initialized_server(backend).await;
    server
        .process_line(&request(
            2,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        ))
        .await
        .expect(expectation)
}

#[tokio::test]
async fn initialize_negotiates_versions_and_enforces_lifecycle() {
    let backend = Backend::success(json!({}));
    let mut server = McpServer::new(&backend);
    let early = server
        .process_line(&request(1, "tools/list", json!({})))
        .await
        .expect("request must receive a response");
    assert_eq!(early["error"]["message"], "Server not initialized");

    let response = server
        .process_line(&request(
            2,
            "initialize",
            json!({ "protocolVersion": "2099-01-01" }),
        ))
        .await
        .expect("initialize must receive a response");
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(response["result"]["serverInfo"]["name"], "blobyard-mcp");
    assert!(response["result"]["capabilities"]["tools"].is_object());

    let repeated = server
        .process_line(&request(
            3,
            "initialize",
            json!({ "protocolVersion": "2025-11-25" }),
        ))
        .await
        .expect("repeated initialize must receive a response");
    assert_eq!(repeated["error"]["message"], "Server already initialized");
}

#[tokio::test]
async fn discovery_returns_namespaced_tools_metadata_and_prompts() {
    let backend = Backend::success(json!({}));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;

    let tools = server
        .process_line(&request(2, "tools/list", json!({})))
        .await
        .expect("tools list must respond");
    let listed = tools["result"]["tools"]
        .as_array()
        .expect("tools must be an array");
    assert!(listed.iter().all(|tool| {
        tool["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("blobyard_"))
    }));
    yard_tests::assert_openapi_catalog(listed);
    yard_tests::assert_dashboard_catalog(listed);
    let download = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_download_file")
        .expect("download tool must be listed");
    assert_eq!(download["annotations"]["destructiveHint"], false);
    let share = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_create_share")
        .expect("share tool must be listed");
    assert_eq!(share["annotations"]["openWorldHint"], true);
    assert_eq!(share["inputSchema"]["additionalProperties"], false);
    let revoke_preview = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_revoke_preview")
        .expect("preview revoke tool must be listed");
    assert_eq!(revoke_preview["annotations"]["destructiveHint"], true);
    let audit = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_list_audit")
        .expect("audit tool must be listed");
    assert_eq!(audit["inputSchema"]["required"], json!(["workspace"]));
    let revoke_session = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_revoke_cli_session")
        .expect("session revoke tool must be listed");
    assert_eq!(revoke_session["annotations"]["destructiveHint"], true);
    yard_tests::assert_yard_catalog(listed);
}

#[tokio::test]
async fn context_discovery_prompts_and_resources_are_usable() {
    let backend = Backend::success(json!({ "identity": "developer" }));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;
    for (method, key, expected) in [
        ("resources/list", "resources", 1),
        ("resources/templates/list", "resourceTemplates", 2),
        ("prompts/list", "prompts", 2),
    ] {
        let response = server
            .process_line(&request(3, method, json!({})))
            .await
            .expect("discovery request must respond");
        assert_eq!(
            response["result"][key].as_array().map(Vec::len),
            Some(expected)
        );
    }
    let resource = server
        .process_line(&request(
            6,
            "resources/read",
            json!({
                "uri": "blobyard://session/identity"
            }),
        ))
        .await
        .expect("resource read must respond");
    assert_eq!(
        resource["result"]["contents"][0]["mimeType"],
        "application/json"
    );
}

#[tokio::test]
async fn prompts_provide_scoped_and_safe_guidance() {
    let backend = Backend::success(json!({}));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;
    for (params, expected) in [
        (
            json!({ "name": "artifact_handoff", "arguments": { "project": "mobile" } }),
            "project `mobile`",
        ),
        (json!({ "name": "artifact_handoff" }), "intended workspace"),
        (
            json!({ "name": "blobyard_get_started" }),
            "durable by default",
        ),
    ] {
        let prompt = server
            .process_line(&request(4, "prompts/get", params))
            .await
            .expect("prompt get must respond");
        assert!(
            prompt["result"]["messages"][0]["content"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(expected))
        );
    }
}

#[tokio::test]
async fn tool_success_is_structured_and_redacts_sensitive_data() {
    let backend = Backend::success(json!({
        "name": "artifact.zip",
        "access_token": "raw-token",
        "nested": { "cookie": "session", "safe": "yes" },
        "shareUrl": "https://blobyard.com/s/raw-capability",
        "inboxUrl": "https://blobyard.com/i/raw-capability",
        "preview_url": "https://preview.blobyard.dev/raw-capability",
        "downloadUrl": "https://r2.example/download",
        "upload_url": "https://r2.example/upload",
        "url": "https://r2.example/object?X-Amz-Signature=secret",
        "message": "line one\nline two"
    }));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;
    let response = server
        .process_line(&request(
            2,
            "tools/call",
            json!({
                "name": "blobyard_list_objects",
                "arguments": { "workspace": "team", "project": "mobile", "versions": true }
            }),
        ))
        .await
        .expect("tool call must respond");
    let data = &response["result"]["structuredContent"]["data"];
    assert_eq!(data["access_token"], "[REDACTED]");
    assert_eq!(data["nested"]["cookie"], "[REDACTED]");
    assert_eq!(data["nested"]["safe"], "yes");
    assert_eq!(data["url"], "[REDACTED]");
    for key in [
        "downloadUrl",
        "upload_url",
        "shareUrl",
        "inboxUrl",
        "preview_url",
    ] {
        assert_eq!(data[key], "[REDACTED]");
    }
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(backend.calls.lock().expect("call log lock").len(), 1);
}

#[tokio::test]
async fn project_resources_dispatch_to_the_backend() {
    let backend = Backend::success(json!({ "safe": true }));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;
    for uri in [
        "blobyard://projects/mobile/objects",
        "blobyard://projects/mobile/retention",
    ] {
        let response = server
            .process_line(&request(2, "resources/read", json!({ "uri": uri })))
            .await
            .expect("project resource must respond");
        assert!(response["result"]["contents"][0]["text"].is_string());
    }
    assert_eq!(backend.calls.lock().expect("call log lock").len(), 2);
}
