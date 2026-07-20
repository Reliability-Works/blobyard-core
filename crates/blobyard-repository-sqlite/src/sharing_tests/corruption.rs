use super::*;

#[test]
fn share_queries_fail_closed_on_corrupt_rows() {
    let fixture = Fixture::new();
    fixture.create();
    super::super::super::execute_corruption(
        &fixture.repository,
        "UPDATE shares SET maximum_downloads = -1 WHERE id = 'share_validation';",
    );
    assert_eq!(
        fixture.repository.list_shares(&fixture.share.workspace_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fixture
            .repository
            .share_by_capability(&fixture.share.capability_hash, 1_001),
        Err(RepositoryError::Unavailable)
    );

    let object_fixture = Fixture::new();
    object_fixture.create();
    super::super::super::execute_corruption(
        &object_fixture.repository,
        "UPDATE object_versions SET source = 'invalid' WHERE id = 'upload_two';",
    );
    assert_eq!(
        object_fixture
            .repository
            .share_by_capability(&object_fixture.share.capability_hash, 1_001),
        Err(RepositoryError::Unavailable)
    );
}
