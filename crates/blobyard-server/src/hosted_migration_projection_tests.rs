#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::test_fixture::{datasets, prepare_fixture, prepare_result};
use super::*;
use blobyard_contract::{ObjectSource, ShareStatus};
use serde_json::json;

#[test]
fn projection_preserves_selected_object_share_and_retention_contracts() {
    let prepared = prepare_fixture(&datasets());

    assert_workspace_and_project(&prepared);
    assert_object(&prepared);
    assert_share(&prepared);
    assert_retention(&prepared);
}

fn assert_workspace_and_project(prepared: &PreparedMigration) {
    assert_eq!(prepared.snapshot.workspaces.len(), 1);
    assert_eq!(prepared.snapshot.workspaces[0].id, "workspace_default");
    assert_eq!(prepared.snapshot.workspaces[0].slug.as_str(), "source");
    assert_eq!(prepared.snapshot.projects.len(), 1);
    assert_eq!(
        prepared.snapshot.projects[0].workspace_id,
        "workspace_default"
    );
}

fn assert_object(prepared: &PreparedMigration) {
    assert_eq!(prepared.snapshot.objects.len(), 1);
    let object = &prepared.snapshot.objects[0];
    assert_eq!(object.object_path, "releases/app.zip");
    assert_eq!(object.version, 7);
    assert_eq!(object.size, 3);
    assert_eq!(object.checksum, "a".repeat(64));
    assert_eq!(object.source, ObjectSource::Ci);
    assert_eq!(object.filename, "app.zip");
    assert_eq!(prepared.source_objects[0].version_id, object.id);
    assert_eq!(
        prepared.source_objects[0].uri,
        "blobyard://source/project/releases/app.zip?version=7"
    );
}

fn assert_share(prepared: &PreparedMigration) {
    let object = &prepared.snapshot.objects[0];
    let share = &prepared.snapshot.shares[0];
    assert_eq!(share.workspace_id, "workspace_default");
    assert_eq!(share.version_id, object.id);
    assert_eq!(share.status, ShareStatus::Active);
    assert_eq!(share.consumed_count, 1);
    assert_eq!(share.maximum_downloads, Some(3));
    assert_eq!(prepared.share_capabilities.len(), 1);
    assert_eq!(
        share.capability_hash,
        crate::auth::hash(prepared.share_capabilities[0].expose_secret())
    );
}

fn assert_retention(prepared: &PreparedMigration) {
    let retention = &prepared.snapshot.retention[0];
    assert_eq!(retention.project_id, prepared.snapshot.projects[0].id);
    assert_eq!(retention.keep_latest, 4);
    assert_eq!(retention.path_glob.as_deref(), Some("releases/**"));
    assert_eq!(retention.branch_glob.as_deref(), Some("main"));
    assert!(retention.enabled);
}

#[test]
fn expired_cloud_share_keeps_policy_without_minting_an_active_url() {
    let mut fixture = datasets();
    fixture.get_mut("shares").expect("shares")[0]["status"] = json!("expired");

    let prepared = prepare_fixture(&fixture);

    assert_eq!(prepared.snapshot.shares[0].status, ShareStatus::Active);
    assert!(prepared.share_capabilities.is_empty());
}

#[test]
fn projection_rejects_missing_selection_and_inconsistent_immutable_uri() {
    assert_eq!(
        prepare(&datasets(), &["missing".to_owned()], &mut |_kind| {
            SecretString::new("bysh_fixture").expect("secret")
        })
        .err(),
        Some(HostedMigrationError::InvalidInput)
    );

    let mut fixture = datasets();
    fixture.get_mut("versions").expect("versions")[0]["uri"] =
        json!("blobyard://source/project/releases/other.zip?version=7");
    assert_eq!(
        prepare(&fixture, &["source".to_owned()], &mut |_kind| {
            SecretString::new("bysh_fixture").expect("secret")
        })
        .err(),
        Some(HostedMigrationError::InvalidExport)
    );
}

#[test]
fn projection_excludes_deleted_and_incomplete_object_versions() {
    let mut fixture = datasets();
    let mut pending = fixture.get("versions").expect("versions")[0].clone();
    pending["versionReference"] = json!("pending-version");
    pending["status"] = json!("pending");
    fixture.get_mut("versions").expect("versions").push(pending);
    fixture.get_mut("objects").expect("objects")[0]["deletedAt"] = json!(99);

    let prepared = prepare_fixture(&fixture);

    assert!(prepared.snapshot.objects.is_empty());
    assert!(prepared.source_objects.is_empty());
    assert!(prepared.snapshot.shares.is_empty());
}

#[test]
fn workspace_selection_rejects_duplicates_invalid_slugs_and_missing_rows() {
    let fixture = datasets();
    for selected in [
        vec!["source".to_owned(), "source".to_owned()],
        vec!["not a slug".to_owned()],
        vec!["source".to_owned(), "missing".to_owned()],
    ] {
        assert_eq!(
            prepare_result(&fixture, &selected).err(),
            Some(HostedMigrationError::InvalidInput)
        );
    }
    let mut deleted = fixture;
    deleted.get_mut("workspace").expect("workspace")[0]["deletedAt"] = json!(1);
    assert_eq!(
        prepare_result(&deleted, &[]).err(),
        Some(HostedMigrationError::InvalidInput)
    );
}

#[test]
fn workspace_and_project_identity_are_deterministic_and_unique() {
    let mut fixture = datasets();
    fixture
        .get_mut("workspace")
        .expect("workspace")
        .push(json!({
            "deletedAt": null,
            "name": "Alpha Workspace",
            "slug": "alpha",
            "workspaceReference": "workspace-alpha"
        }));
    fixture.get_mut("projects").expect("projects").push(json!({
        "deletedAt": null,
        "name": "Alpha Project",
        "projectReference": "project-alpha",
        "slug": "alpha",
        "workspaceReference": "workspace-source"
    }));
    let prepared = prepare_result(&fixture, &[]).expect("expanded projection");
    assert_eq!(prepared.snapshot.workspaces.len(), 2);
    assert_eq!(prepared.snapshot.workspaces[0].slug.as_str(), "alpha");
    assert_eq!(prepared.snapshot.workspaces[0].id, "workspace_default");
    assert!(prepared.snapshot.workspaces[1].id.starts_with("workspace_"));
    assert_eq!(prepared.snapshot.projects.len(), 2);

    let mut duplicate_workspace = fixture.clone();
    duplicate_workspace.get_mut("workspace").expect("workspace")[1]["workspaceReference"] =
        json!("workspace-source");
    assert_eq!(
        prepare_result(&duplicate_workspace, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let mut duplicate_project = fixture;
    duplicate_project.get_mut("projects").expect("projects")[1]["projectReference"] =
        json!("project-source");
    assert_eq!(
        prepare_result(&duplicate_project, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );
}
