use crate::adapter::yard_validation;
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardFile, RepositoryError, YardDeployRecord, YardDeployStatus,
};

fn deploy(status: YardDeployStatus) -> YardDeployRecord {
    let deploy = super::validation_start::deploy();
    YardDeployRecord {
        id: deploy.id,
        yard_id: deploy.yard_id,
        workspace_id: deploy.workspace_id,
        project_id: deploy.project_id,
        client_deploy_id: deploy.client_deploy_id,
        manifest_root: deploy.manifest_root,
        deployment_host_label: deploy.deployment_host_label,
        spa: deploy.spa,
        clean_urls: deploy.clean_urls,
        status,
        created_at_ms: deploy.created_at_ms,
        finalised_at_ms: None,
        file_count: 0,
        total_bytes: 0,
    }
}

fn file(path: &str, version: &str, bytes: u64) -> NewYardFile {
    NewYardFile {
        normalized_path: path.to_owned(),
        version_id: version.to_owned(),
        byte_size: bytes,
    }
}

fn action_event(at: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_action".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "yard.deleted".to_owned(),
        request_id: "request_action".to_owned(),
        target_type: "web_yard".to_owned(),
        metadata: vec![(
            "yardId".to_owned(),
            AuditValue::String("yard_fixture".to_owned()),
        )],
        created_at_ms: at,
    }
}

#[test]
fn finalise_validation_requires_an_incomplete_deploy_and_indexed_manifest() {
    let uploading = deploy(YardDeployStatus::Uploading);
    let index = file("index.html", "version_index", 5);
    assert_eq!(
        yard_validation::finalise(&uploading, std::slice::from_ref(&index), 20),
        Ok((20, 1, 5))
    );
    assert_eq!(
        yard_validation::finalise(&deploy(YardDeployStatus::Finalising), &[index], 20),
        Ok((20, 1, 5))
    );
    assert_eq!(
        yard_validation::finalise(&deploy(YardDeployStatus::Live), &[], 20),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        yard_validation::finalise(&uploading, &[], 20),
        Err(RepositoryError::Conflict)
    );
    let oversized = vec![file("index.html", "version_index", 0); 10_001];
    assert_eq!(
        yard_validation::finalise(&uploading, &oversized, 20),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn finalise_validation_rejects_unsafe_duplicate_or_incomplete_files() {
    let deploy = deploy(YardDeployStatus::Uploading);
    for files in [
        vec![file("index.html", "", 1)],
        vec![file("../index.html", "version_index", 1)],
        vec![
            file("index.html", "version_one", 1),
            file("index.html", "version_two", 1),
        ],
        vec![file("asset.js", "version_asset", 1)],
    ] {
        assert_eq!(
            yard_validation::finalise(&deploy, &files, 20),
            Err(RepositoryError::InvalidInput)
        );
    }
    let overflow = [
        file("index.html", "version_index", u64::MAX),
        file("asset.js", "version_asset", 1),
    ];
    assert_eq!(
        yard_validation::finalise(&deploy, &overflow, 20),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_validation::finalise(
            &deploy,
            &[file("index.html", "version_index", i64::MAX as u64 + 1,)],
            20,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        yard_validation::finalise(
            &deploy,
            &[file("index.html", "version_index", 1)],
            i64::MAX as u64 + 1,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn failure_validation_enforces_state_code_message_and_time_contracts() {
    assert_eq!(
        yard_validation::failure(
            &deploy(YardDeployStatus::Uploading),
            "UPLOAD_FAILED",
            "The upload failed.",
            20,
        ),
        Ok(20)
    );
    assert_eq!(
        yard_validation::failure(
            &deploy(YardDeployStatus::Live),
            "UPLOAD_FAILED",
            "The upload failed.",
            20,
        ),
        Err(RepositoryError::Conflict)
    );
    for code in ["A", "lowercase", "BAD-CODE"] {
        assert_eq!(
            yard_validation::failure(&deploy(YardDeployStatus::Uploading), code, "Failed.", 20,),
            Err(RepositoryError::InvalidInput)
        );
    }
    for message in ["", "bad\nmessage"] {
        assert_eq!(
            yard_validation::failure(
                &deploy(YardDeployStatus::Uploading),
                "UPLOAD_FAILED",
                message,
                20,
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        yard_validation::failure(
            &deploy(YardDeployStatus::Uploading),
            "UPLOAD_FAILED",
            "Failed.",
            i64::MAX as u64 + 1,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn action_event_validation_matches_metadata_and_sqlite_time_range() {
    let event = action_event(20);
    let metadata = [("yardId", AuditValue::String("yard_fixture".to_owned()))];
    assert_eq!(
        yard_validation::action_event(
            &event,
            "yard.deleted",
            "web_yard",
            "workspace_fixture",
            20,
            metadata.clone(),
        ),
        Ok(20)
    );
    assert_eq!(
        yard_validation::action_event(
            &event,
            "yard.rolled_back",
            "yard_deploy",
            "workspace_fixture",
            20,
            metadata.clone(),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let overflow = action_event(i64::MAX as u64 + 1);
    assert_eq!(
        yard_validation::action_event(
            &overflow,
            "yard.deleted",
            "web_yard",
            "workspace_fixture",
            i64::MAX as u64 + 1,
            metadata,
        ),
        Err(RepositoryError::InvalidInput)
    );
}
