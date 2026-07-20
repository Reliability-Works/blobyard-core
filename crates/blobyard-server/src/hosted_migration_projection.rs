use super::HostedMigrationError;
use blobyard_contract::MigrationSnapshot;
use blobyard_core::{GeneratedSecretKind, SecretString};
use serde::Deserialize;
use std::collections::BTreeMap;

#[path = "hosted_migration_projection_identities.rs"]
mod identities;
#[path = "hosted_migration_projection_objects.rs"]
mod objects;
#[path = "hosted_migration_projection_policies.rs"]
mod policies;

use identities::{ProjectMap, WorkspaceMap, select_objects, select_projects, select_workspaces};
use objects::select_versions;
use policies::{select_retention, select_shares};

pub(super) struct PreparedMigration {
    pub(super) snapshot: MigrationSnapshot,
    pub(super) source_objects: Vec<SourceObject>,
    pub(super) share_capabilities: Vec<SecretString>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourceObject {
    pub(super) version_id: String,
    pub(super) uri: String,
    pub(super) size: u64,
    pub(super) checksum: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportWorkspace {
    deleted_at: Option<u64>,
    name: String,
    slug: String,
    workspace_reference: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportProject {
    deleted_at: Option<u64>,
    name: String,
    project_reference: String,
    slug: String,
    workspace_reference: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportObject {
    deleted_at: Option<u64>,
    filename: String,
    logical_path: String,
    object_reference: String,
    project_reference: String,
    workspace_reference: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportVersion {
    byte_size: u64,
    checksum_sha256: String,
    content_type: String,
    created_at: u64,
    deleted_at: Option<u64>,
    git_branch: Option<String>,
    git_commit: Option<String>,
    git_repository: Option<String>,
    object_reference: String,
    project_reference: String,
    source: String,
    status: String,
    uri: String,
    version: u64,
    version_reference: String,
    workspace_reference: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportShare {
    consumed_count: u64,
    created_at: u64,
    expires_at: u64,
    maximum_downloads: Option<u64>,
    object_version_reference: String,
    revoked_at: Option<u64>,
    share_reference: String,
    status: String,
    workspace_reference: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRetention {
    branch_glob: Option<String>,
    created_at: u64,
    enabled: bool,
    keep_latest: u32,
    path_glob: Option<String>,
    project_reference: String,
    updated_at: u64,
}

struct ProjectionMaps {
    workspaces: WorkspaceMap,
    projects: ProjectMap,
    versions: BTreeMap<String, String>,
}

pub(super) fn prepare(
    datasets: &BTreeMap<String, Vec<serde_json::Value>>,
    selected_slugs: &[String],
    generate: &mut dyn FnMut(GeneratedSecretKind) -> SecretString,
) -> Result<PreparedMigration, HostedMigrationError> {
    let source_workspaces = records::<ExportWorkspace>(datasets, "workspace")?;
    let source_projects = records::<ExportProject>(datasets, "projects")?;
    let source_objects = records::<ExportObject>(datasets, "objects")?;
    let source_versions = records::<ExportVersion>(datasets, "versions")?;
    let source_shares = records::<ExportShare>(datasets, "shares")?;
    let source_retention = records::<ExportRetention>(datasets, "retention_policies")?;

    let (workspaces, workspace_records) = select_workspaces(source_workspaces, selected_slugs)?;
    let (projects, project_records) = select_projects(source_projects, &workspaces)?;
    let objects = select_objects(source_objects, &workspaces, &projects)?;
    let (versions, object_records, download_records) =
        select_versions(source_versions, &workspaces, &projects, &objects)?;
    let maps = ProjectionMaps {
        workspaces,
        projects,
        versions,
    };
    let (shares, share_capabilities) = select_shares(source_shares, &maps, generate)?;
    let retention = select_retention(source_retention, &maps.projects)?;
    Ok(PreparedMigration {
        snapshot: MigrationSnapshot {
            workspaces: workspace_records,
            projects: project_records,
            objects: object_records,
            shares,
            retention,
        },
        source_objects: download_records,
        share_capabilities,
    })
}

fn records<T: for<'de> Deserialize<'de>>(
    datasets: &BTreeMap<String, Vec<serde_json::Value>>,
    dataset: &str,
) -> Result<Vec<T>, HostedMigrationError> {
    datasets
        .get(dataset)
        .ok_or(HostedMigrationError::InvalidExport)?
        .iter()
        .cloned()
        .map(|value| {
            serde_json::from_value(value).map_err(|_error| HostedMigrationError::InvalidExport)
        })
        .collect()
}

#[cfg(test)]
#[path = "hosted_migration_projection_test_fixture.rs"]
mod test_fixture;

#[cfg(test)]
#[path = "hosted_migration_projection_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hosted_migration_projection_edge_tests.rs"]
mod edge_tests;
