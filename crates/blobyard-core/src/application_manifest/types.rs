use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Canonical application runtime identifier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ApplicationRuntime {
    /// Blob Yard's first JavaScript and TypeScript runtime contract.
    #[serde(rename = "blobyard-js-1")]
    BlobyardJs1,
}

impl ApplicationRuntime {
    /// Returns the stable manifest representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BlobyardJs1 => "blobyard-js-1",
        }
    }
}

/// Required application identity and runtime selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestApplication {
    /// DNS-label application name.
    pub name: String,
    /// Selected runtime contract.
    pub runtime: ApplicationRuntime,
}

/// Optional static frontend configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFrontend {
    /// Portable relative asset directory.
    pub directory: String,
    /// Whether unmatched routes fall back to the root entry file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spa_fallback: Option<bool>,
    /// Whether extensionless paths resolve matching HTML files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean_urls: Option<bool>,
}

/// Application role and permission configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestAuth {
    /// Role assigned when no more specific mapping exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role: Option<String>,
    /// Declared roles, ordered by stable role name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<BTreeMap<String, ManifestRole>>,
}

/// One declared application role.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestRole {
    /// Other declared roles whose permissions this role inherits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<Vec<String>>,
    /// Application permissions granted to this role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
}

/// Per-environment relational database declaration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDatabase {
    /// Portable relative migrations directory.
    pub migrations: String,
}

/// Canonical, versioned Blob Yard application manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationManifest {
    /// Manifest schema version. Version 1 is currently supported.
    pub schema_version: u8,
    /// Required application identity and runtime.
    pub application: ManifestApplication,
    /// Optional static frontend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend: Option<ManifestFrontend>,
    /// Optional application role model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ManifestAuth>,
    /// Optional relational database.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<ManifestDatabase>,
    /// Declared object buckets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buckets: Option<Vec<super::Bucket>>,
    /// Declared isolated functions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<super::Function>>,
    /// Declared scheduled jobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<super::Job>>,
    /// Declared HTTP routes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<super::Route>>,
    /// Optional runtime limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<super::Limits>,
    /// Optional staged health check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<super::Health>,
}
