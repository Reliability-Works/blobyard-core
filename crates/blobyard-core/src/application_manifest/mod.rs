#![allow(
    clippy::redundant_pub_crate,
    reason = "the private manifest module shares validators with sibling domain modules"
)]

mod cron;
mod cross;
mod cross_roles;
mod error;
pub(crate) mod patterns;
mod resources;
mod schema;
mod schema_execution;
mod schema_helpers;
mod schema_runtime;
mod types;

use serde::Serialize;
use std::collections::BTreeSet;

pub use error::{ManifestError, ManifestErrors};
pub use resources::{
    Backoff, Bucket, BucketVisibility, DatabaseAccess, Function, FunctionClass, FunctionType,
    Health, HttpMethod, Job, Limits, Retry, Route, RouteAuth,
};
pub use types::{
    ApplicationManifest, ApplicationRuntime, ManifestApplication, ManifestAuth, ManifestDatabase,
    ManifestFrontend, ManifestRole,
};

/// Counts of capabilities declared by an application manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCapabilityCounts {
    /// Declared application roles.
    pub roles: usize,
    /// Whether a relational database is declared.
    pub databases: usize,
    /// Declared object buckets.
    pub buckets: usize,
    /// Declared isolated functions.
    pub functions: usize,
    /// Declared scheduled jobs.
    pub jobs: usize,
    /// Declared HTTP routes.
    pub routes: usize,
    /// Distinct secret names requested by functions.
    pub secrets: usize,
    /// Distinct network targets requested by functions.
    pub network_targets: usize,
    /// Functions requesting email authority.
    pub email_functions: usize,
}

impl ApplicationManifest {
    /// Parses TOML and enforces the version 1 schema and cross-field contract.
    ///
    /// # Errors
    ///
    /// Returns every independently detectable validation failure with a precise field path.
    pub fn parse_toml(source: &str) -> Result<Self, ManifestErrors> {
        let value = toml::from_str::<toml::Value>(source)
            .map_err(|error| ManifestErrors::one("$", format!("invalid TOML: {error}")))?;
        let errors = schema::validate(&value);
        if !errors.is_empty() {
            return Err(ManifestErrors::new(errors));
        }
        Self::parse_validated(value)
    }

    fn parse_validated(value: toml::Value) -> Result<Self, ManifestErrors> {
        let manifest = decode(value)?;
        let errors = cross::validate(&manifest);
        if errors.is_empty() {
            Ok(manifest)
        } else {
            Err(ManifestErrors::new(errors))
        }
    }

    /// Serializes the deterministic canonical JSON projection used for hashing.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the in-memory manifest cannot be represented as JSON.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Counts declared capabilities without broadening any function's individual grants.
    #[must_use]
    pub fn capability_counts(&self) -> ManifestCapabilityCounts {
        let functions = self.functions.as_deref().unwrap_or_default();
        ManifestCapabilityCounts {
            roles: self
                .auth
                .as_ref()
                .and_then(|auth| auth.roles.as_ref())
                .map_or(0, std::collections::BTreeMap::len),
            databases: usize::from(self.database.is_some()),
            buckets: self.buckets.as_deref().map_or(0, <[Bucket]>::len),
            functions: functions.len(),
            jobs: self.jobs.as_deref().map_or(0, <[Job]>::len),
            routes: self.routes.as_deref().map_or(0, <[Route]>::len),
            secrets: distinct(functions, |function| function.secrets.as_deref()),
            network_targets: distinct(functions, |function| function.network.as_deref()),
            email_functions: functions
                .iter()
                .filter(|function| function.email == Some(true))
                .count(),
        }
    }
}

fn decode(value: toml::Value) -> Result<ApplicationManifest, ManifestErrors> {
    value.try_into().map_err(|error| {
        ManifestErrors::one("$", format!("manifest could not be decoded: {error}"))
    })
}

fn distinct<'a>(
    functions: &'a [Function],
    select: impl Fn(&'a Function) -> Option<&'a [String]>,
) -> usize {
    functions
        .iter()
        .filter_map(select)
        .flatten()
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests;
