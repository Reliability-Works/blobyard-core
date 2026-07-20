use super::support::*;

#[test]
fn inactive_yards_reject_rollback() {
    let (_temporary, repository, _version_id, _size) = repository();
    let first_yard = yard("rollback", 1);
    assert!(start(&repository, &first_yard, &deploy(&first_yard, 1, true)).is_ok());
    suspend_yard(&repository, &first_yard.id);
    assert_eq!(
        repository.rollback_web_yard(
            &first_yard.id,
            None,
            2,
            &action(
                "yard.rolled_back",
                "yard_deploy",
                "yardId",
                &first_yard.id,
                2,
            ),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn invalid_failure_and_corrupt_rollback_history_fail_closed() {
    let fixture = FinaliseFixture::new("corrupt-history");
    assert_eq!(
        fixture
            .repository
            .fail_yard_deploy(&fixture.deploy.id, "invalid-code", "Failed.", 2,),
        Err(RepositoryError::InvalidInput)
    );
    assert!(fixture.finalise(3).is_ok());
    let _second = fixture.finalise_replacement(2, 4);
    let connection = success(fixture.repository.test_connection());
    assert!(
        connection
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .is_ok()
    );
    assert!(
        connection
            .execute(
                "UPDATE yard_deploys SET status = 'corrupt' WHERE id = ?1",
                [&fixture.deploy.id],
            )
            .is_ok()
    );
    drop(connection);
    assert_eq!(
        fixture.repository.rollback_web_yard(
            &fixture.yard.id,
            None,
            5,
            &action(
                "yard.rolled_back",
                "yard_deploy",
                "yardId",
                &fixture.yard.id,
                5,
            ),
        ),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn invalid_lifecycle_audit_leaves_an_active_yard_unchanged() {
    let fixture = FinaliseFixture::new("rollback-audit");
    assert!(fixture.finalise(3).is_ok());
    let _second = fixture.finalise_replacement(2, 4);
    assert_eq!(
        fixture.repository.rollback_web_yard(
            &fixture.yard.id,
            Some(&fixture.deploy.id),
            5,
            &created(&fixture.yard.id, 5),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .delete_web_yard(&fixture.yard.id, 6, &created(&fixture.yard.id, 6),),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        success(fixture.repository.web_yard_by_id(&fixture.yard.id)).status,
        WebYardStatus::Active
    );
}
