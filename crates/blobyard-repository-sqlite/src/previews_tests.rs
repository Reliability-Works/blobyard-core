#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::{MetadataRepository, ProjectRecord};
use blobyard_core::Slug;

#[path = "previews_tests/corruption.rs"]
mod corruption;
#[path = "previews_tests/revocation.rs"]
mod revocation;

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: SqliteRepository,
    preview: NewPreview,
}

impl Fixture {
    fn new() -> Self {
        let (temporary, repository) = super::super::repository_with_transfers();
        Self {
            _temporary: temporary,
            repository,
            preview: NewPreview {
                id: "preview_validation".to_owned(),
                workspace_id: "workspace_fixture".to_owned(),
                project_id: "project_fixture".to_owned(),
                capability_hash: "9".repeat(64),
                expires_at_ms: 5_000,
                created_at_ms: 1_000,
                files: vec![NewPreviewFile {
                    normalized_path: "index.html".to_owned(),
                    version_id: "upload_two".to_owned(),
                }],
            },
        }
    }

    fn create(&self) -> PreviewRecord {
        self.repository
            .create_preview(
                &self.preview,
                &event("preview.created", &self.preview, 1_000),
            )
            .expect("preview")
    }
}

fn event(action: &str, preview: &NewPreview, created_at_ms: u64) -> NewAuditEvent {
    blobyard_testkit::preview_event(action, &preview.id, created_at_ms)
}

fn assert_invalid(fixture: &Fixture, mutate: impl FnOnce(&mut NewPreview)) {
    let mut preview = fixture.preview.clone();
    mutate(&mut preview);
    assert_eq!(
        fixture.repository.create_preview(
            &preview,
            &event("preview.created", &preview, preview.created_at_ms),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn creation_rejects_invalid_identity_bounds_manifest_and_audit() {
    let fixture = Fixture::new();
    for mutate in [
        |preview: &mut NewPreview| preview.id.clear(),
        |preview: &mut NewPreview| preview.workspace_id.clear(),
        |preview: &mut NewPreview| preview.project_id.clear(),
        |preview: &mut NewPreview| preview.capability_hash = "invalid".to_owned(),
        |preview: &mut NewPreview| preview.expires_at_ms = preview.created_at_ms,
        |preview: &mut NewPreview| preview.files.clear(),
        |preview: &mut NewPreview| preview.files[0].normalized_path = "page.html".to_owned(),
        |preview: &mut NewPreview| preview.files[0].version_id.clear(),
        |preview: &mut NewPreview| preview.expires_at_ms = u64::MAX,
        |preview: &mut NewPreview| preview.created_at_ms = u64::MAX,
    ] {
        assert_invalid(&fixture, mutate);
    }
    for path in [
        "",
        "bad\nname.html",
        "/index.html",
        "index.html/",
        "dir//index.html",
        "./index.html",
        "dir/../index.html",
        "dir\\index.html",
    ] {
        assert_invalid(&fixture, |preview| {
            preview.files.push(NewPreviewFile {
                normalized_path: path.to_owned(),
                version_id: "upload_two".to_owned(),
            });
        });
    }
    assert_invalid(&fixture, |preview| {
        preview.files.push(preview.files[0].clone());
    });
    assert_invalid(&fixture, |preview| {
        preview.files = (0..=MAXIMUM_PREVIEW_FILES)
            .map(|index| NewPreviewFile {
                normalized_path: if index == 0 {
                    "index.html".to_owned()
                } else {
                    format!("{index}.html")
                },
                version_id: "upload_two".to_owned(),
            })
            .collect();
    });
    assert_eq!(
        fixture.repository.create_preview(
            &fixture.preview,
            &event("preview.wrong", &fixture.preview, 1_000),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn creation_rejects_foreign_projects_and_unavailable_versions_atomically() {
    let fixture = Fixture::new();
    for version_id in ["missing", "upload_abort_requested"] {
        assert_unavailable_version(&fixture, version_id);
    }

    let mut foreign_workspace = fixture.preview.clone();
    foreign_workspace.workspace_id = "workspace_foreign".to_owned();
    let mut foreign_event = event(
        "preview.created",
        &foreign_workspace,
        foreign_workspace.created_at_ms,
    );
    foreign_event.workspace_id = foreign_workspace.workspace_id.clone();
    assert_eq!(
        fixture
            .repository
            .create_preview(&foreign_workspace, &foreign_event),
        Err(RepositoryError::NotFound)
    );
    assert!(
        fixture
            .repository
            .list_previews("project_fixture")
            .expect("list")
            .is_empty()
    );

    fixture
        .repository
        .create_project(&ProjectRecord {
            id: "project_foreign".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign".to_owned()).expect("slug"),
        })
        .expect("foreign project");
    let mut foreign = fixture.preview.clone();
    foreign.project_id = "project_foreign".to_owned();
    assert_eq!(
        fixture.repository.create_preview(
            &foreign,
            &event("preview.created", &foreign, foreign.created_at_ms),
        ),
        Err(RepositoryError::NotFound)
    );
    assert!(
        fixture
            .repository
            .list_previews("project_foreign")
            .expect("list")
            .is_empty()
    );
}

fn assert_unavailable_version(fixture: &Fixture, version_id: &str) {
    let mut preview = fixture.preview.clone();
    version_id.clone_into(&mut preview.files[0].version_id);
    assert_eq!(
        fixture.repository.create_preview(
            &preview,
            &event("preview.created", &preview, preview.created_at_ms),
        ),
        Err(RepositoryError::NotFound)
    );
    assert!(
        fixture
            .repository
            .list_previews("project_fixture")
            .expect("list")
            .is_empty()
    );
}

#[test]
fn queries_reject_invalid_inputs_and_preserve_exact_snapshot() {
    let fixture = Fixture::new();
    let created = fixture.create();
    assert_eq!(
        fixture.repository.preview_by_id(&created.id),
        Ok(created.clone())
    );
    assert_eq!(
        fixture
            .repository
            .preview_file_by_capability(&fixture.preview.capability_hash, "index.html", 4_999,)
            .map(|target| target.preview),
        Ok(created)
    );
    assert_eq!(
        fixture.repository.list_previews(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.preview_by_id(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .preview_file_by_capability("invalid", "index.html", 1_001),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.preview_file_by_capability(
            &fixture.preview.capability_hash,
            "/index.html",
            1_001,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.preview_file_by_capability(
            &fixture.preview.capability_hash,
            "index.html",
            u64::MAX,
        ),
        Err(RepositoryError::InvalidInput)
    );
}
