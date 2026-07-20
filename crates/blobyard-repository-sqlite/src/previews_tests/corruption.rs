use super::*;

impl super::super::super::CapabilityCorruptionFixture for Fixture {
    type ListedRecord = PreviewRecord;
    type ResolvedRecord = PreviewTarget;

    fn repository(&self) -> &SqliteRepository {
        &self.repository
    }

    fn list(&self) -> Result<Vec<Self::ListedRecord>, RepositoryError> {
        self.repository.list_previews(&self.preview.project_id)
    }

    fn resolve(&self) -> Result<Self::ResolvedRecord, RepositoryError> {
        self.repository.preview_file_by_capability(
            &self.preview.capability_hash,
            "index.html",
            1_001,
        )
    }
}

#[test]
fn preview_queries_fail_closed_on_corrupt_rows() {
    for (corruption, capability_error) in [
        (
            "UPDATE previews SET expires_at_ms = -1 WHERE id = 'preview_validation';",
            RepositoryError::NotFound,
        ),
        (
            "UPDATE previews SET status = 'invalid' WHERE id = 'preview_validation';",
            RepositoryError::NotFound,
        ),
        (
            "UPDATE previews SET revoked_at_ms = -1 WHERE id = 'preview_validation';",
            RepositoryError::Unavailable,
        ),
    ] {
        let fixture = Fixture::new();
        fixture.create();
        super::super::super::assert_capability_corruption(&fixture, corruption, capability_error);
    }

    let object_fixture = Fixture::new();
    object_fixture.create();
    super::super::super::execute_corruption(
        &object_fixture.repository,
        "UPDATE object_versions SET source = 'invalid' WHERE id = 'upload_two';",
    );
    assert_eq!(
        object_fixture.repository.preview_file_by_capability(
            &object_fixture.preview.capability_hash,
            "index.html",
            1_001,
        ),
        Err(RepositoryError::Unavailable)
    );
}
