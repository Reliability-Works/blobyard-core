use super::{ExportObject, ExportProject, ExportWorkspace, HostedMigrationError};
use blobyard_contract::{ProjectRecord, WorkspaceRecord};
use blobyard_core::Slug;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(super) type WorkspaceMap = BTreeMap<String, (String, ExportWorkspace)>;
pub(super) type ProjectMap = BTreeMap<String, (String, ExportProject)>;
type SelectedWorkspaces = (WorkspaceMap, Vec<WorkspaceRecord>);
type SelectedProjects = (ProjectMap, Vec<ProjectRecord>);

pub(super) fn select_workspaces(
    records: Vec<ExportWorkspace>,
    selected_slugs: &[String],
) -> Result<SelectedWorkspaces, HostedMigrationError> {
    let requested = selected_slugs.iter().cloned().collect::<BTreeSet<_>>();
    if requested.len() != selected_slugs.len()
        || requested.iter().any(|slug| Slug::new(slug).is_err())
    {
        return Err(HostedMigrationError::InvalidInput);
    }
    let mut active = records
        .into_iter()
        .filter(|workspace| workspace.deleted_at.is_none())
        .filter(|workspace| requested.is_empty() || requested.contains(&workspace.slug))
        .collect::<Vec<_>>();
    active.sort_by(|left, right| left.slug.cmp(&right.slug));
    if active.is_empty()
        || (!requested.is_empty()
            && active
                .iter()
                .map(|row| &row.slug)
                .collect::<BTreeSet<_>>()
                .len()
                != requested.len())
    {
        return Err(HostedMigrationError::InvalidInput);
    }
    let mut map = BTreeMap::new();
    let mut destination = Vec::new();
    for (index, workspace) in active.into_iter().enumerate() {
        let id = if index == 0 {
            "workspace_default".to_owned()
        } else {
            stable_id("workspace", &workspace.workspace_reference)
        };
        let slug = Slug::new(workspace.slug.clone())
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        destination.push(WorkspaceRecord {
            id: id.clone(),
            name: workspace.name.clone(),
            slug,
        });
        if map
            .insert(workspace.workspace_reference.clone(), (id, workspace))
            .is_some()
        {
            return Err(HostedMigrationError::InvalidExport);
        }
    }
    Ok((map, destination))
}

pub(super) fn select_projects(
    records: Vec<ExportProject>,
    workspaces: &WorkspaceMap,
) -> Result<SelectedProjects, HostedMigrationError> {
    let mut selected = records
        .into_iter()
        .filter(|project| project.deleted_at.is_none())
        .filter_map(|project| {
            let workspace_id = workspaces.get(&project.workspace_reference)?.0.clone();
            Some((project, workspace_id))
        })
        .collect::<Vec<_>>();
    selected.sort_by(|(left, _), (right, _)| {
        left.workspace_reference
            .cmp(&right.workspace_reference)
            .then_with(|| left.slug.cmp(&right.slug))
    });
    let mut map = BTreeMap::new();
    let mut destination = Vec::new();
    for (project, workspace_id) in selected {
        let id = stable_id("project", &project.project_reference);
        let slug = Slug::new(project.slug.clone())
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        destination.push(ProjectRecord {
            id: id.clone(),
            workspace_id,
            name: project.name.clone(),
            slug,
        });
        if map
            .insert(project.project_reference.clone(), (id, project))
            .is_some()
        {
            return Err(HostedMigrationError::InvalidExport);
        }
    }
    Ok((map, destination))
}

pub(super) fn select_objects(
    records: Vec<ExportObject>,
    workspaces: &WorkspaceMap,
    projects: &ProjectMap,
) -> Result<BTreeMap<String, ExportObject>, HostedMigrationError> {
    let mut selected = BTreeMap::new();
    for (object, project_workspace) in records
        .into_iter()
        .filter(|object| object.deleted_at.is_none())
        .filter_map(|object| {
            workspaces.get(&object.workspace_reference)?;
            let project_workspace = projects
                .get(&object.project_reference)?
                .1
                .workspace_reference
                .clone();
            Some((object, project_workspace))
        })
    {
        if project_workspace != object.workspace_reference
            || selected
                .insert(object.object_reference.clone(), object)
                .is_some()
        {
            return Err(HostedMigrationError::InvalidExport);
        }
    }
    Ok(selected)
}

pub(super) fn stable_id(prefix: &str, reference: &str) -> String {
    format!("{prefix}_{}", stable_digest(reference))
}

pub(super) fn stable_digest(reference: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"blobyard-hosted-migration\0");
    digest.update(reference.as_bytes());
    blobyard_core::hex_digest(&digest.finalize())
}
