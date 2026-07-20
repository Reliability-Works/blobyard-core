#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use serde_json::{Value, json};

pub(super) fn datasets() -> BTreeMap<String, Vec<Value>> {
    BTreeMap::from([
        workspace_dataset(),
        project_dataset(),
        object_dataset(),
        version_dataset(),
        share_dataset(),
        retention_dataset(),
    ])
}

fn workspace_dataset() -> (String, Vec<Value>) {
    (
        "workspace".to_owned(),
        vec![json!({
            "deletedAt": null,
            "name": "Source Workspace",
            "slug": "source",
            "workspaceReference": "workspace-source"
        })],
    )
}

fn project_dataset() -> (String, Vec<Value>) {
    (
        "projects".to_owned(),
        vec![json!({
            "deletedAt": null,
            "name": "Project",
            "projectReference": "project-source",
            "slug": "project",
            "workspaceReference": "workspace-source"
        })],
    )
}

fn object_dataset() -> (String, Vec<Value>) {
    (
        "objects".to_owned(),
        vec![json!({
            "deletedAt": null,
            "filename": "app.zip",
            "logicalPath": "releases/app.zip",
            "objectReference": "object-source",
            "projectReference": "project-source",
            "workspaceReference": "workspace-source"
        })],
    )
}

fn version_dataset() -> (String, Vec<Value>) {
    (
        "versions".to_owned(),
        vec![json!({
            "byteSize": 3,
            "checksumSha256": "a".repeat(64),
            "contentType": "application/zip",
            "createdAt": 1000,
            "deletedAt": null,
            "gitBranch": "main",
            "gitCommit": "b".repeat(40),
            "gitRepository": "Reliability-Works/example",
            "objectReference": "object-source",
            "projectReference": "project-source",
            "source": "ci",
            "status": "ready",
            "uri": "blobyard://source/project/releases/app.zip?version=7",
            "version": 7,
            "versionReference": "version-source",
            "workspaceReference": "workspace-source"
        })],
    )
}

fn share_dataset() -> (String, Vec<Value>) {
    (
        "shares".to_owned(),
        vec![json!({
            "consumedCount": 1,
            "createdAt": 2000,
            "expiresAt": 4000,
            "maximumDownloads": 3,
            "objectVersionReference": "version-source",
            "revokedAt": null,
            "shareReference": "share-source",
            "status": "active",
            "workspaceReference": "workspace-source"
        })],
    )
}

fn retention_dataset() -> (String, Vec<Value>) {
    (
        "retention_policies".to_owned(),
        vec![json!({
            "branchGlob": "main",
            "createdAt": 2100,
            "enabled": true,
            "keepLatest": 4,
            "pathGlob": "releases/**",
            "projectReference": "project-source",
            "updatedAt": 2200
        })],
    )
}

pub(super) fn prepare_fixture(datasets: &BTreeMap<String, Vec<Value>>) -> PreparedMigration {
    let mut index = 0_u32;
    let mut generate = |_kind| {
        index += 1;
        SecretString::new(format!("bysh_fixture_{index}")).expect("secret")
    };
    prepare(datasets, &["source".to_owned()], &mut generate).expect("prepared migration")
}

pub(super) fn prepare_result(
    datasets: &BTreeMap<String, Vec<Value>>,
    selected: &[String],
) -> Result<PreparedMigration, HostedMigrationError> {
    let mut generate = |_kind| SecretString::new("bysh_fixture").expect("secret");
    prepare(datasets, selected, &mut generate)
}

pub(super) fn source_maps() -> (WorkspaceMap, ProjectMap, BTreeMap<String, ExportObject>) {
    let fixture = datasets();
    let workspaces = select_workspaces(
        records::<ExportWorkspace>(&fixture, "workspace").expect("workspaces"),
        &["source".to_owned()],
    )
    .expect("selected workspaces")
    .0;
    let projects = select_projects(
        records::<ExportProject>(&fixture, "projects").expect("projects"),
        &workspaces,
    )
    .expect("selected projects")
    .0;
    let objects = select_objects(
        records::<ExportObject>(&fixture, "objects").expect("objects"),
        &workspaces,
        &projects,
    )
    .expect("selected objects");
    (workspaces, projects, objects)
}
