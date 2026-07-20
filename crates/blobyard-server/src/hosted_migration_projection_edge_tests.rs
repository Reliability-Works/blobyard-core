#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::test_fixture::{datasets, prepare_result, source_maps};
use super::*;
use blobyard_contract::ShareStatus;
use serde_json::json;

#[test]
fn malformed_export_records_and_object_relationships_fail_closed() {
    for dataset in [
        "workspace",
        "projects",
        "objects",
        "versions",
        "shares",
        "retention_policies",
    ] {
        let mut missing = datasets();
        missing.remove(dataset);
        assert_eq!(
            prepare_result(&missing, &[]).err(),
            Some(HostedMigrationError::InvalidExport),
            "missing {dataset}"
        );
    }

    let mut malformed = datasets();
    malformed.get_mut("projects").expect("projects")[0]["name"] = json!(7);
    assert_eq!(
        prepare_result(&malformed, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let (workspaces, projects, _objects) = source_maps();
    let mut duplicate = records::<ExportObject>(&datasets(), "objects").expect("objects");
    duplicate.push(duplicate[0].clone());
    assert_eq!(
        select_objects(duplicate, &workspaces, &projects).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    for (dataset, field) in [("workspace", "slug"), ("projects", "slug")] {
        let mut invalid_slug = datasets();
        invalid_slug.get_mut(dataset).expect("dataset")[0][field] = json!("not a slug");
        assert_eq!(
            prepare_result(&invalid_slug, &[]).err(),
            Some(HostedMigrationError::InvalidExport),
            "invalid {dataset} slug"
        );
    }

    let mut mismatched_object = datasets();
    mismatched_object.get_mut("objects").expect("objects")[0]["workspaceReference"] =
        json!("workspace-other");
    mismatched_object
        .get_mut("workspace")
        .expect("workspaces")
        .push(json!({
            "deletedAt": null,
            "name": "Other",
            "slug": "other",
            "workspaceReference": "workspace-other"
        }));
    assert_eq!(
        prepare_result(&mismatched_object, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );
}

#[test]
fn foreign_projects_and_objects_are_excluded_without_partial_projection() {
    let mut fixture = datasets();
    fixture.get_mut("projects").expect("projects").push(json!({
        "deletedAt": null,
        "name": "Foreign",
        "projectReference": "project-foreign",
        "slug": "foreign",
        "workspaceReference": "workspace-missing"
    }));
    for (object_reference, workspace_reference, project_reference) in [
        (
            "object-foreign-workspace",
            "workspace-missing",
            "project-source",
        ),
        (
            "object-foreign-project",
            "workspace-source",
            "project-missing",
        ),
    ] {
        fixture.get_mut("objects").expect("objects").push(json!({
            "deletedAt": null,
            "filename": "ignored.zip",
            "logicalPath": "ignored.zip",
            "objectReference": object_reference,
            "projectReference": project_reference,
            "workspaceReference": workspace_reference
        }));
    }

    let prepared = prepare_result(&fixture, &[]).expect("filtered projection");
    assert_eq!(prepared.snapshot.projects.len(), 1);
    assert_eq!(prepared.snapshot.objects.len(), 1);
}

#[test]
fn version_projection_rejects_invalid_source_checksum_and_duplicate_identity() {
    let (workspaces, projects, objects) = source_maps();
    let fixture = datasets();
    let valid = records::<ExportVersion>(&fixture, "versions").expect("versions");
    let mut duplicate = valid.clone();
    duplicate.push(valid[0].clone());
    assert_eq!(
        select_versions(duplicate, &workspaces, &projects, &objects).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let mutations: [fn(&mut ExportVersion); 7] = [
        |value| value.source = "unknown".to_owned(),
        |value| value.checksum_sha256 = "A".repeat(64),
        |value| value.checksum_sha256 = "a".repeat(63),
        |value| value.version = 0,
        |value| value.uri = "not-a-blobyard-uri".to_owned(),
        |value| value.workspace_reference = "missing-workspace".to_owned(),
        |value| value.project_reference = "missing-project".to_owned(),
    ];
    for mutate in mutations {
        let mut versions = valid.clone();
        mutate(&mut versions[0]);
        assert_eq!(
            select_versions(versions, &workspaces, &projects, &objects).err(),
            Some(HostedMigrationError::InvalidExport)
        );
    }
}

#[test]
fn share_states_duplicate_identity_and_retention_identity_are_exact() {
    let fixture = datasets();
    for (status, expected) in [
        ("exhausted", ShareStatus::Exhausted),
        ("revoked", ShareStatus::Revoked),
    ] {
        let mut candidate = fixture.clone();
        candidate.get_mut("shares").expect("shares")[0]["status"] = json!(status);
        let prepared = prepare_result(&candidate, &[]).expect("share state");
        assert_eq!(prepared.snapshot.shares[0].status, expected);
        assert!(prepared.share_capabilities.is_empty());
    }

    let mut invalid = fixture.clone();
    invalid.get_mut("shares").expect("shares")[0]["status"] = json!("unknown");
    assert_eq!(
        prepare_result(&invalid, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let mut duplicate_share = fixture.clone();
    let share = duplicate_share.get("shares").expect("shares")[0].clone();
    duplicate_share
        .get_mut("shares")
        .expect("shares")
        .push(share);
    assert_eq!(
        prepare_result(&duplicate_share, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let mut duplicate_retention = fixture;
    let retention = duplicate_retention
        .get("retention_policies")
        .expect("retention")[0]
        .clone();
    duplicate_retention
        .get_mut("retention_policies")
        .expect("retention")
        .push(retention);
    assert_eq!(
        prepare_result(&duplicate_retention, &[]).err(),
        Some(HostedMigrationError::InvalidExport)
    );
}
