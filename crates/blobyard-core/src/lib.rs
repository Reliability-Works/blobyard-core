//! Shared domain primitives for Blobyard clients and services.

mod application_manifest;
mod application_policy;
mod encoding;
mod error;
mod secret;
mod slug;
mod uri;
mod web_yard_origin;

pub use application_manifest::{
    ApplicationManifest, ApplicationRuntime, Backoff, Bucket, BucketVisibility, DatabaseAccess,
    Function, FunctionClass, FunctionType, Health, HttpMethod, Job, Limits, ManifestApplication,
    ManifestAuth, ManifestCapabilityCounts, ManifestDatabase, ManifestError, ManifestErrors,
    ManifestFrontend, ManifestRole, Retry, Route, RouteAuth,
};
pub use application_policy::{
    ApplicationPolicyError, ApplicationPolicyGraph, ApplicationRoleDefinition,
    CanonicalApplicationPolicy, EffectiveApplicationPolicy, MAXIMUM_APPLICATION_POLICY_ROLES,
    MAXIMUM_DIRECT_INHERITED_ROLES, MAXIMUM_DIRECT_ROLE_PERMISSIONS,
    canonicalize_application_policy, valid_source_manifest_digest,
};
pub use encoding::hex_digest;
pub use error::{BlobyardError, ErrorCode};
pub use secret::{GeneratedSecretKind, SecretString};
pub use slug::{Slug, SlugError};
pub use uri::{BlobyardUri, BlobyardUriError};
pub use web_yard_origin::{CLOUD_WEB_YARD_ORIGIN, WebYardOrigin, is_valid_dns_label};
