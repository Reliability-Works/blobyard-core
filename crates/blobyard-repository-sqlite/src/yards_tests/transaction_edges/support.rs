pub(super) use super::super::{present, success};
pub(super) use crate::adapter::{SqliteRepository, yard_history};
pub(super) use blobyard_contract::{
    AuditValue, LifecycleRepository, NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile,
    RepositoryError, TransferRepository, WebYardRepository, WebYardStatus, YardDeployStatus,
    YardDeploymentRecord,
};
use blobyard_core::Slug;

pub(super) fn repository() -> (tempfile::TempDir, SqliteRepository, String, u64) {
    let temporary = success(tempfile::tempdir());
    let repository = success(SqliteRepository::open(
        &temporary.path().join("metadata.sqlite3"),
    ));
    success(blobyard_testkit::repository_conformance(&repository));
    success(blobyard_testkit::transfer_conformance(
        &repository,
        "project_fixture",
    ));
    let mut objects = success(repository.list_stored_objects(
        "project_fixture",
        Some("artifacts/build.zip"),
        false,
    ));
    let object = present(objects.pop());
    let size = present(object.version.size);
    (temporary, repository, object.version.id, size)
}

pub(in crate::adapter::yards::tests) fn yard(name: &str, number: u64) -> NewWebYard {
    NewWebYard {
        id: format!("yard_{name}_{number}"),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: success(Slug::new(name)),
        host_label: format!("{name}-123456789-fixture-{number}"),
        created_at_ms: number,
    }
}

pub(in crate::adapter::yards::tests) fn deploy(
    yard: &NewWebYard,
    number: u64,
    spa: bool,
) -> NewYardDeploy {
    let client = format!("clientdeploy{number:08}");
    NewYardDeploy {
        id: format!("deploy_{}_{}", yard.name, number),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: client.clone(),
        manifest_root: format!(".blobyard-yard/{}/{client}/", yard.id),
        deployment_host_label: format!("{}-0123456789-fixture-{number}", yard.name),
        spa,
        clean_urls: spa,
        created_at_ms: number,
    }
}

fn event(
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    at: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{at}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}_{at}"),
        target_type: target_type.to_owned(),
        metadata,
        created_at_ms: at,
    }
}

pub(in crate::adapter::yards::tests) fn created(yard_id: &str, at: u64) -> NewAuditEvent {
    event(
        "yard.created",
        "web_yard",
        vec![("yardId".to_owned(), AuditValue::String(yard_id.to_owned()))],
        at,
    )
}

pub(in crate::adapter::yards::tests) fn deployed(
    deploy_id: &str,
    files: u64,
    bytes: u64,
    status: &str,
    at: u64,
) -> NewAuditEvent {
    event(
        "yard.deployed",
        "yard_deploy",
        vec![
            (
                "deployId".to_owned(),
                AuditValue::String(deploy_id.to_owned()),
            ),
            ("fileCount".to_owned(), AuditValue::Number(files)),
            ("status".to_owned(), AuditValue::String(status.to_owned())),
            ("totalBytes".to_owned(), AuditValue::Number(bytes)),
        ],
        at,
    )
}

pub(super) fn action(
    action: &str,
    target_type: &str,
    key: &str,
    value: &str,
    at: u64,
) -> NewAuditEvent {
    event(
        action,
        target_type,
        vec![(key.to_owned(), AuditValue::String(value.to_owned()))],
        at,
    )
}

pub(super) fn start(
    repository: &SqliteRepository,
    yard: &NewWebYard,
    deploy: &NewYardDeploy,
) -> Result<(), RepositoryError> {
    repository
        .start_yard_deploy(yard, deploy, &created(&yard.id, yard.created_at_ms))
        .map(|_record| ())
}

pub(super) fn suspend_yard(repository: &SqliteRepository, yard_id: &str) {
    let connection = success(repository.test_connection());
    assert!(
        connection
            .execute(
                "UPDATE web_yards SET status = 'suspended' WHERE id = ?1",
                [yard_id],
            )
            .is_ok()
    );
    drop(connection);
}

pub(super) struct FinaliseFixture {
    pub(super) _temporary: tempfile::TempDir,
    pub(super) repository: SqliteRepository,
    pub(super) yard: NewWebYard,
    pub(super) deploy: NewYardDeploy,
    pub(super) file: [NewYardFile; 1],
    pub(super) size: u64,
}

impl FinaliseFixture {
    pub(super) fn new(name: &str) -> Self {
        let (temporary, repository, version_id, size) = repository();
        let yard = yard(name, 1);
        let deploy = deploy(&yard, 1, false);
        success(start(&repository, &yard, &deploy));
        Self {
            _temporary: temporary,
            repository,
            yard,
            deploy,
            file: [NewYardFile {
                normalized_path: "index.html".to_owned(),
                version_id,
                byte_size: size,
            }],
            size,
        }
    }

    pub(super) fn finalise(&self, at: u64) -> Result<YardDeploymentRecord, RepositoryError> {
        self.repository.finalise_yard_deploy(
            &self.deploy.id,
            &self.file,
            at,
            &deployed(&self.deploy.id, 1, self.size, "live", at),
        )
    }

    pub(super) fn finalise_replacement(&self, number: u64, at: u64) -> YardDeploymentRecord {
        let replacement = deploy(&self.yard, number, true);
        success(start(&self.repository, &self.yard, &replacement));
        success(self.repository.finalise_yard_deploy(
            &replacement.id,
            &self.file,
            at,
            &deployed(&replacement.id, 1, self.size, "live", at),
        ))
    }
}
