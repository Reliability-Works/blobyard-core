use crate::ToolCall;
use serde_json::Value;
use std::{future::Future, pin::Pin};

/// A boxed backend operation future tied to the backend reference.
pub type BackendFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, BackendError>> + Send + 'a>>;

/// A safe, user-facing failure returned by a tool backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendError {
    code: String,
    message: String,
}

impl BackendError {
    /// Creates an error with a stable code and a message safe for model context.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub(crate) fn as_value(&self) -> Value {
        serde_json::json!({ "code": self.code, "message": self.message })
    }
}

/// Executes validated Blobyard tool calls through the host's existing authorization path.
pub trait ToolBackend: Send + Sync {
    /// Runs one validated tool call and returns JSON-safe, non-secret output.
    fn call(&self, call: ToolCall) -> BackendFuture<'_>;
}
