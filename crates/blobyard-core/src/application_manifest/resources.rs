use serde::{Deserialize, Serialize};

/// Object bucket visibility.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BucketVisibility {
    /// Objects require an authorized capability.
    Private,
    /// Objects may be read publicly.
    PublicRead,
}

/// One per-environment object bucket.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bucket {
    /// DNS-label bucket name.
    pub name: String,
    /// Optional object visibility, private by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<BucketVisibility>,
    /// Optional maximum object size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_object_size: Option<String>,
}

/// Isolated function invocation type.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionType {
    /// Typed browser SDK invocation.
    Rpc,
    /// Declared HTTP route invocation.
    Http,
    /// Externally verified webhook invocation.
    Webhook,
    /// Named event invocation.
    Event,
    /// Scheduled job invocation.
    Scheduled,
    /// Named queue invocation.
    Queue,
}

/// Relational database authority granted to one function.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseAccess {
    /// No database authority.
    None,
    /// Read-only database authority.
    Read,
    /// Read and write database authority.
    ReadWrite,
}

/// One isolated JavaScript or TypeScript function and its capability ceiling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    /// DNS-label function name.
    pub name: String,
    /// Portable JavaScript or TypeScript module path.
    pub entry: String,
    /// Invocation type.
    #[serde(rename = "type")]
    pub function_type: FunctionType,
    /// Application permissions required by this function.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    /// Database authority for this function only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseAccess>,
    /// Bucket grants for this function only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buckets: Option<Vec<String>>,
    /// Secret names exposed to this function only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Vec<String>>,
    /// Network targets reachable by this function only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Vec<String>>,
    /// Whether this function may send email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<bool>,
    /// Event name required by event functions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Queue name required by queue functions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
}

/// Retry backoff strategy for a scheduled job.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backoff {
    /// Retry after a fixed delay.
    Fixed,
    /// Increase the delay between attempts.
    Exponential,
}

/// Optional scheduled job retry policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Retry {
    /// Maximum total attempts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u8>,
    /// Delay strategy between attempts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff: Option<Backoff>,
}

/// One durable scheduled job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    /// DNS-label job name.
    pub name: String,
    /// Referenced scheduled function name.
    pub function: String,
    /// Five-field cron schedule.
    pub schedule: String,
    /// Canonical IANA timezone identifier.
    pub timezone: String,
    /// Optional retry policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<Retry>,
}

/// HTTP method supported by application routes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HttpMethod {
    /// GET.
    GET,
    /// HEAD.
    HEAD,
    /// POST.
    POST,
    /// PUT.
    PUT,
    /// PATCH.
    PATCH,
    /// DELETE.
    DELETE,
}

/// Route authentication policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteAuth {
    /// A Yard session is required.
    Required,
    /// The route is publicly callable.
    Public,
}

/// One declared HTTP route.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Absolute normalized route path.
    pub path: String,
    /// HTTP method.
    pub method: HttpMethod,
    /// Referenced function name.
    pub function: String,
    /// Optional route authentication policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<RouteAuth>,
}

/// Runtime resource class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionClass {
    /// Standard isolated runtime class.
    Standard,
}

/// Optional application runtime limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Optional function resource class.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_class: Option<FunctionClass>,
    /// Optional per-invocation timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_timeout: Option<String>,
    /// Optional maximum concurrent invocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u8>,
}

/// Optional staged release health check.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Health {
    /// Referenced health function name.
    pub function: String,
    /// Optional health check timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}
