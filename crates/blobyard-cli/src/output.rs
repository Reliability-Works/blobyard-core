use crate::{ConfigSource, GlobalArgs};
use blobyard_core::BlobyardError;
use serde_json::Value;
use std::fmt::Write as _;

/// Effective command output mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    /// Human-readable standard output.
    Human,
    /// One stable JSON document on standard output.
    Json,
    /// No non-essential success output.
    Quiet,
}

/// Output controls derived from global flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputOptions {
    mode: OutputMode,
    verbose: bool,
}

impl OutputOptions {
    /// Derives output behavior from parsed flags.
    #[must_use]
    pub const fn from_flags(flags: &GlobalArgs) -> Self {
        let mode = if flags.json {
            OutputMode::Json
        } else if flags.quiet {
            OutputMode::Quiet
        } else {
            OutputMode::Human
        };
        Self {
            mode,
            verbose: flags.verbose,
        }
    }

    /// Returns the selected mode.
    #[must_use]
    pub const fn mode(self) -> OutputMode {
        self.mode
    }

    /// Returns whether redacted diagnostics are enabled.
    #[must_use]
    pub const fn verbose(self) -> bool {
        self.verbose
    }
}

/// Redaction-safe configuration diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    api_base_url: Option<String>,
    api_source: Option<ConfigSource>,
    workspace_source: Option<ConfigSource>,
    project_source: Option<ConfigSource>,
    token_source: Option<&'static str>,
}

impl Diagnostics {
    /// Adds the validated API endpoint and its source.
    #[must_use]
    pub fn with_api(mut self, base_url: impl Into<String>, source: ConfigSource) -> Self {
        self.api_base_url = Some(base_url.into());
        self.api_source = Some(source);
        self
    }

    /// Adds workspace and project selection sources.
    #[must_use]
    pub const fn with_scope(
        mut self,
        workspace: Option<ConfigSource>,
        project: Option<ConfigSource>,
    ) -> Self {
        self.workspace_source = workspace;
        self.project_source = project;
        self
    }

    /// Adds only a credential source label, never credential material.
    #[must_use]
    pub const fn with_token_source(mut self, source: &'static str) -> Self {
        self.token_source = Some(source);
        self
    }

    fn lines(&self, request_id: Option<&str>) -> String {
        let mut output = String::new();
        push_optional(&mut output, "api", self.api_base_url.as_deref());
        push_source(&mut output, "api_source", self.api_source);
        push_source(&mut output, "workspace_source", self.workspace_source);
        push_source(&mut output, "project_source", self.project_source);
        push_optional(&mut output, "token_source", self.token_source);
        push_optional(&mut output, "request_id", request_id);
        output
    }
}

/// Successful command payload and human presentation.
pub struct CommandResult {
    data: Value,
    human: String,
    request_id: Option<String>,
    partial_error: Option<BlobyardError>,
}

impl CommandResult {
    /// Creates a successful result.
    #[must_use]
    pub fn new(data: Value, human: impl Into<String>, request_id: Option<String>) -> Self {
        Self {
            data,
            human: human.into(),
            request_id,
            partial_error: None,
        }
    }

    /// Creates a local success without a server request identifier.
    #[must_use]
    pub fn local(data: Value, human: impl Into<String>) -> Self {
        Self::new(data, human, None)
    }

    /// Creates a result that preserves partial data and exits with a failure status.
    #[must_use]
    pub fn partial_failure(data: Value, human: impl Into<String>, error: BlobyardError) -> Self {
        Self {
            data,
            human: human.into(),
            request_id: error.request_id().map(ToOwned::to_owned),
            partial_error: Some(error),
        }
    }

    pub(crate) fn into_data(self) -> Value {
        self.data
    }
}

impl std::fmt::Debug for CommandResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandResult")
            .field("has_data", &!self.data.is_null())
            .field("has_human_output", &!self.human.is_empty())
            .field("request_id", &self.request_id)
            .field("has_partial_error", &self.partial_error.is_some())
            .finish()
    }
}

/// Fully rendered process output and exit status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedOutput {
    /// Standard output bytes as UTF-8 text.
    pub stdout: String,
    /// Standard error bytes as UTF-8 text.
    pub stderr: String,
    /// Process exit status.
    pub exit_code: u8,
}

/// Renders stable success and failure contracts.
#[derive(Clone, Debug)]
pub struct OutputRenderer {
    options: OutputOptions,
    diagnostics: Diagnostics,
    warnings: Vec<String>,
}

impl OutputRenderer {
    /// Creates a renderer.
    #[must_use]
    pub const fn new(options: OutputOptions, diagnostics: Diagnostics) -> Self {
        Self {
            options,
            diagnostics,
            warnings: Vec::new(),
        }
    }

    /// Adds an essential warning that is always emitted on standard error.
    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Renders a successful command.
    #[must_use]
    pub fn success(&self, result: CommandResult) -> RenderedOutput {
        let stdout = match self.options.mode() {
            OutputMode::Json => result.partial_error.as_ref().map_or_else(
                || success_json(&result.data, result.request_id.as_deref()),
                |error| partial_failure_json(&result.data, error),
            ),
            OutputMode::Quiet => String::new(),
            OutputMode::Human => terminate_line(result.human),
        };
        let mut stderr = warning_lines(&self.warnings);
        if self.options.verbose() {
            stderr.push_str(&self.diagnostics.lines(result.request_id.as_deref()));
        }
        RenderedOutput {
            stdout,
            stderr,
            exit_code: result
                .partial_error
                .as_ref()
                .map_or(0, |error| error.code().exit_code()),
        }
    }

    /// Renders a safe failure with its documented exit class.
    #[must_use]
    pub fn failure(&self, error: &BlobyardError) -> RenderedOutput {
        let stdout = if self.options.mode() == OutputMode::Json {
            failure_json(error)
        } else {
            String::new()
        };
        let mut stderr = warning_lines(&self.warnings);
        if self.options.mode() != OutputMode::Json {
            let _ = writeln!(stderr, "{error}");
        }
        if self.options.verbose() {
            stderr.push_str(&self.diagnostics.lines(error.request_id()));
        }
        RenderedOutput {
            stdout,
            stderr,
            exit_code: error.code().exit_code(),
        }
    }
}

fn success_json(data: &Value, request_id: Option<&str>) -> String {
    format!(
        "{{\"ok\":true,\"data\":{data},\"requestId\":{}}}\n",
        json_string(request_id)
    )
}

fn failure_json(error: &BlobyardError) -> String {
    let code = Value::String(error.code().as_str().to_owned());
    let message = Value::String(error.message().to_owned());
    format!(
        "{{\"ok\":false,\"error\":{{\"code\":{code},\"message\":{message}}},\"requestId\":{}}}\n",
        json_string(error.request_id())
    )
}

fn partial_failure_json(data: &Value, error: &BlobyardError) -> String {
    let code = Value::String(error.code().as_str().to_owned());
    let message = Value::String(error.message().to_owned());
    format!(
        "{{\"ok\":false,\"data\":{data},\"error\":{{\"code\":{code},\"message\":{message}}},\"requestId\":{}}}\n",
        json_string(error.request_id())
    )
}

fn json_string(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.to_owned()))
}

fn terminate_line(mut value: String) -> String {
    if !value.is_empty() && !value.ends_with('\n') {
        value.push('\n');
    }
    value
}

fn warning_lines(warnings: &[String]) -> String {
    warnings.iter().fold(String::new(), |mut output, warning| {
        let _ = writeln!(output, "{warning}");
        output
    })
}

fn push_source(output: &mut String, name: &str, source: Option<ConfigSource>) {
    push_optional(output, name, source.map(ConfigSource::as_str));
}

fn push_optional(output: &mut String, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        let _ = writeln!(output, "diagnostic {name}={value}");
    }
}
