use super::*;

impl super::super::super::CapabilityCorruptionFixture for Fixture {
    type ListedRecord = InboxRecord;
    type ResolvedRecord = InboxRecord;

    fn repository(&self) -> &SqliteRepository {
        &self.repository
    }

    fn list(&self) -> Result<Vec<Self::ListedRecord>, RepositoryError> {
        self.repository.list_inboxes(&self.inbox.project_id)
    }

    fn resolve(&self) -> Result<Self::ResolvedRecord, RepositoryError> {
        self.repository
            .inbox_by_capability(&self.inbox.capability_hash, 1_001)
    }
}

#[test]
fn inbox_queries_fail_closed_on_corrupt_rows() {
    for (corruption, capability_error) in [
        (
            "UPDATE inboxes SET maximum_files = -1 WHERE id = 'inbox_validation';",
            RepositoryError::Unavailable,
        ),
        (
            "UPDATE inboxes SET status = 'invalid' WHERE id = 'inbox_validation';",
            RepositoryError::NotFound,
        ),
        (
            "UPDATE inboxes SET revoked_at_ms = -1 WHERE id = 'inbox_validation';",
            RepositoryError::Unavailable,
        ),
    ] {
        let fixture = Fixture::new();
        fixture.create();
        super::super::super::assert_capability_corruption(&fixture, corruption, capability_error);
    }
}
