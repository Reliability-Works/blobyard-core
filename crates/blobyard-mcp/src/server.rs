#[path = "catalog.rs"]
mod catalog;
#[path = "context.rs"]
mod context;
#[path = "sanitize.rs"]
mod sanitize;

use crate::{ToolBackend, ToolCall};
use sanitize::{IssuedCapability, sanitize};
use serde_json::{Map, Value, json};
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_VERSIONS: [&str; 4] = [PROTOCOL_VERSION, "2025-06-18", "2025-03-26", "2024-11-05"];

/// Stateful JSON-RPC request processor for one MCP connection.
pub struct McpServer<'a, B: ToolBackend + ?Sized> {
    backend: &'a B,
    initialized: bool,
}

impl<'a, B: ToolBackend + ?Sized> McpServer<'a, B> {
    /// Creates a server backed by the host's authorized Blobyard operations.
    #[must_use]
    pub const fn new(backend: &'a B) -> Self {
        Self {
            backend,
            initialized: false,
        }
    }

    /// Processes one line and returns a response for requests or `None` for notifications.
    pub async fn process_line(&mut self, line: &str) -> Option<Value> {
        let message = match serde_json::from_str::<Value>(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(error_response(
                    Value::Null,
                    -32_700,
                    "Parse error",
                    Some(json!({ "detail": error.to_string() })),
                ));
            }
        };
        let Some(request) = message.as_object() else {
            return Some(error_response(
                Value::Null,
                -32_600,
                "Invalid Request",
                None,
            ));
        };
        self.process_request(request).await
    }

    async fn process_request(&mut self, request: &Map<String, Value>) -> Option<Value> {
        let id = request.get("id").cloned();
        if request.get("jsonrpc") != Some(&Value::String("2.0".to_owned())) {
            return id.map(|id| error_response(id, -32_600, "Invalid Request", None));
        }
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            return id.map(|id| error_response(id, -32_600, "Invalid Request", None));
        };
        let id = id?;
        if !matches!(id, Value::String(_) | Value::Number(_)) {
            return Some(error_response(
                Value::Null,
                -32_600,
                "Invalid Request",
                None,
            ));
        }
        Some(self.request(id, method, request.get("params")).await)
    }

    async fn request(&mut self, id: Value, method: &str, params: Option<&Value>) -> Value {
        match method {
            "initialize" => self.initialize(id, params),
            "ping" => success(id, json!({})),
            _ if !self.initialized => error_response(id, -32_600, "Server not initialized", None),
            "tools/list" => success(id, json!({ "tools": catalog::tools() })),
            "tools/call" => self.call_tool(id, params).await,
            "resources/list" => success(id, context::resources()),
            "resources/templates/list" => success(id, context::resource_templates()),
            "resources/read" => self.read_resource(id, params).await,
            "prompts/list" => success(id, context::prompts()),
            "prompts/get" => prompt_response(id, params),
            _ => error_response(
                id,
                -32_601,
                "Method not found",
                Some(json!({ "method": method })),
            ),
        }
    }

    fn initialize(&mut self, id: Value, params: Option<&Value>) -> Value {
        if self.initialized {
            return error_response(id, -32_600, "Server already initialized", None);
        }
        let requested = params
            .and_then(Value::as_object)
            .and_then(|fields| fields.get("protocolVersion"))
            .and_then(Value::as_str);
        let Some(requested) = requested else {
            return error_response(
                id,
                -32_602,
                "Invalid params",
                Some(json!({ "detail": "protocolVersion is required" })),
            );
        };
        let negotiated = if SUPPORTED_VERSIONS.contains(&requested) {
            requested
        } else {
            PROTOCOL_VERSION
        };
        self.initialized = true;
        success(
            id,
            json!({
                "protocolVersion": negotiated,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "blobyard-mcp",
                    "title": "Blobyard",
                    "version": env!("CARGO_PKG_VERSION"),
                    "description": "Authorized Blobyard file sharing tools."
                },
                "instructions": "Use only blobyard_* tools. Confirm mutation targets and lifetimes. Never request or expose tokens, credentials, cookies, OTPs, provider secrets, or signed URLs."
            }),
        )
    }

    async fn call_tool(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(fields) = params.and_then(Value::as_object) else {
            return error_response(
                id,
                -32_602,
                "Invalid params",
                Some(json!({ "detail": "params must be an object" })),
            );
        };
        let Some(name) = fields.get("name").and_then(Value::as_str) else {
            return error_response(
                id,
                -32_602,
                "Invalid params",
                Some(json!({ "detail": "tool name is required" })),
            );
        };
        let arguments = fields
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let call = match ToolCall::parse(name, &arguments) {
            Ok(call) => call,
            Err(detail) => {
                return error_response(
                    id,
                    -32_602,
                    "Invalid params",
                    Some(json!({ "detail": detail })),
                );
            }
        };
        let issued = IssuedCapability::for_call(&call);
        match self.backend.call(call).await {
            Ok(mut output) => {
                sanitize(&mut output, issued);
                success(id, tool_result(&output, false))
            }
            Err(error) => {
                let mut output = error.as_value();
                sanitize(&mut output, IssuedCapability::None);
                success(id, tool_result(&output, true))
            }
        }
    }

    async fn read_resource(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(uri) = params
            .and_then(Value::as_object)
            .and_then(|fields| fields.get("uri"))
            .and_then(Value::as_str)
        else {
            return error_response(id, -32_602, "Invalid params", None);
        };
        let Some(call) = context::resource_call(uri) else {
            return error_response(
                id,
                -32_002,
                "Resource not found",
                Some(json!({ "uri": uri })),
            );
        };
        match self.backend.call(call).await {
            Ok(mut output) => {
                sanitize(&mut output, IssuedCapability::None);
                let text = output.to_string();
                success(
                    id,
                    json!({ "contents": [{
                        "uri": uri, "mimeType": "application/json", "text": text
                    }] }),
                )
            }
            Err(error) => {
                let mut data = error.as_value();
                sanitize(&mut data, IssuedCapability::None);
                error_response(id, -32_603, "Resource backend error", Some(data))
            }
        }
    }
}

/// Serves newline-delimited MCP messages over asynchronous streams.
///
/// # Errors
///
/// Returns an I/O error when reading, serializing, or writing a protocol message fails.
pub async fn serve<R, W, B>(mut reader: R, mut writer: W, backend: &B) -> io::Result<()>
where
    R: AsyncBufRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
    B: ToolBackend,
{
    serve_stream(&mut reader, &mut writer, backend).await
}

async fn serve_stream(
    reader: &mut (dyn AsyncBufRead + Unpin + Send),
    writer: &mut (dyn AsyncWrite + Unpin + Send),
    backend: &dyn ToolBackend,
) -> io::Result<()> {
    let mut lines = reader.lines();
    let mut server = McpServer::new(backend);
    while let Some(line) = lines.next_line().await? {
        if let Some(response) = server.process_line(&line).await {
            let bytes = response.to_string().into_bytes();
            writer.write_all(&bytes).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
    Ok(())
}

/// Serves MCP on standard input and standard output.
///
/// # Errors
///
/// Returns an I/O error when standard input or standard output fails.
pub async fn serve_stdio<B: ToolBackend>(backend: &B) -> io::Result<()> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::stdout();
    serve_stream(&mut reader, &mut writer, backend).await
}

fn prompt_response(id: Value, params: Option<&Value>) -> Value {
    let Some(fields) = params.and_then(Value::as_object) else {
        return error_response(id, -32_602, "Invalid params", None);
    };
    let Some(name) = fields.get("name").and_then(Value::as_str) else {
        return error_response(id, -32_602, "Invalid prompt name", None);
    };
    let project = fields
        .get("arguments")
        .and_then(Value::as_object)
        .and_then(|arguments| arguments.get("project"))
        .and_then(Value::as_str);
    match context::prompt(name, project) {
        Some(prompt) => success(id, prompt),
        None => error_response(id, -32_602, "Invalid prompt name", None),
    }
}

fn tool_result(output: &Value, is_error: bool) -> Value {
    let text = output.to_string();
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": { "data": output },
        "isError": is_error
    })
}

fn success(id: Value, result: Value) -> Value {
    let mut response = Map::new();
    response.insert("jsonrpc".to_owned(), Value::String("2.0".to_owned()));
    response.insert("id".to_owned(), id);
    response.insert("result".to_owned(), result);
    Value::Object(response)
}

fn error_response(id: Value, code: i32, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let (Some(fields), Some(data)) = (error.as_object_mut(), data) {
        fields.insert("data".to_owned(), data);
    }
    let mut response = Map::new();
    response.insert("jsonrpc".to_owned(), Value::String("2.0".to_owned()));
    response.insert("id".to_owned(), id);
    response.insert("error".to_owned(), error);
    Value::Object(response)
}
