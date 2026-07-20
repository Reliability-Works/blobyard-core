use super::identities::{ProjectMap, WorkspaceMap, stable_digest, stable_id};
use super::{ExportObject, ExportVersion, HostedMigrationError, SourceObject};
use blobyard_contract::{MigrationObjectRecord, ObjectSource};
use blobyard_core::BlobyardUri;
use std::collections::BTreeMap;
use std::str::FromStr;

type SelectedVersions = (
    BTreeMap<String, String>,
    Vec<MigrationObjectRecord>,
    Vec<SourceObject>,
);

pub(super) fn select_versions(
    records: Vec<ExportVersion>,
    workspaces: &WorkspaceMap,
    projects: &ProjectMap,
    objects: &BTreeMap<String, ExportObject>,
) -> Result<SelectedVersions, HostedMigrationError> {
    let mut selected = records
        .into_iter()
        .filter(|version| version.deleted_at.is_none() && version.status == "ready")
        .filter_map(|version| {
            let object = objects.get(&version.object_reference)?.clone();
            Some((version, object))
        })
        .collect::<Vec<_>>();
    selected.sort_by(|(left, _), (right, _)| left.uri.cmp(&right.uri));
    let mut map = BTreeMap::new();
    let mut destination = Vec::new();
    let mut downloads = Vec::new();
    for (version, object) in selected {
        validate_version_relations(&version, &object, workspaces, projects)?;
        let id = stable_id("version", &version.version_reference);
        let checksum = checked_checksum(&version.checksum_sha256)?;
        let source =
            ObjectSource::parse(&version.source).ok_or(HostedMigrationError::InvalidExport)?;
        destination.push(MigrationObjectRecord {
            id: id.clone(),
            project_id: projects[&version.project_reference].0.clone(),
            object_path: object.logical_path.clone(),
            version: version.version,
            storage_key: format!("migration/{}", stable_digest(&version.version_reference)),
            size: version.byte_size,
            checksum: checksum.clone(),
            created_at_ms: version.created_at,
            source,
            git_repository: version.git_repository,
            git_commit: version.git_commit,
            git_branch: version.git_branch,
            filename: object.filename.clone(),
            content_type: version.content_type,
        });
        downloads.push(SourceObject {
            version_id: id.clone(),
            uri: version.uri,
            size: version.byte_size,
            checksum,
        });
        if map.insert(version.version_reference, id).is_some() {
            return Err(HostedMigrationError::InvalidExport);
        }
    }
    Ok((map, destination, downloads))
}

fn validate_version_relations(
    version: &ExportVersion,
    object: &ExportObject,
    workspaces: &WorkspaceMap,
    projects: &ProjectMap,
) -> Result<(), HostedMigrationError> {
    let workspace = &workspaces
        .get(&version.workspace_reference)
        .ok_or(HostedMigrationError::InvalidExport)?
        .1;
    let project = &projects
        .get(&version.project_reference)
        .ok_or(HostedMigrationError::InvalidExport)?
        .1;
    let uri = BlobyardUri::from_str(&version.uri)
        .map_err(|_error| HostedMigrationError::InvalidExport)?;
    if object.workspace_reference == version.workspace_reference
        && object.project_reference == version.project_reference
        && project.workspace_reference == version.workspace_reference
        && uri.workspace() == workspace.slug
        && uri.project() == project.slug
        && uri.logical_path() == object.logical_path
        && uri.version().map(u64::from) == Some(version.version)
        && version.version > 0
    {
        Ok(())
    } else {
        Err(HostedMigrationError::InvalidExport)
    }
}

fn checked_checksum(value: &str) -> Result<String, HostedMigrationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value.to_owned())
    } else {
        Err(HostedMigrationError::InvalidExport)
    }
}
