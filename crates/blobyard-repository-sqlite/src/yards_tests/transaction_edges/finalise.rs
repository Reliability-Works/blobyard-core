use super::support::*;

#[test]
fn rejects_missing_versions_and_invalid_audit() {
    let fixture = FinaliseFixture::new("docs");
    assert_eq!(
        fixture.repository.finalise_yard_deploy(
            &fixture.deploy.id,
            &[],
            10,
            &deployed(&fixture.deploy.id, 0, 0, "live", 10),
        ),
        Err(RepositoryError::Conflict)
    );
    let missing = [NewYardFile {
        normalized_path: "index.html".to_owned(),
        version_id: "version_missing".to_owned(),
        byte_size: fixture.size,
    }];
    assert_eq!(
        fixture.repository.finalise_yard_deploy(
            &fixture.deploy.id,
            &missing,
            10,
            &deployed(&fixture.deploy.id, 1, fixture.size, "live", 10),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.finalise_yard_deploy(
            &fixture.deploy.id,
            &fixture.file,
            10,
            &created(&fixture.yard.id, 10),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn rolls_back_a_suppressed_stable_promotion() {
    let fixture = FinaliseFixture::new("suppressed");
    let connection = success(fixture.repository.test_connection());
    assert!(
        connection
            .execute_batch(
                "CREATE TRIGGER suppress_yard_promotion BEFORE UPDATE OF current_deploy_id ON web_yards BEGIN SELECT RAISE(IGNORE); END;",
            )
            .is_ok()
    );
    drop(connection);
    assert_eq!(fixture.finalise(10), Err(RepositoryError::Conflict));
    let connection = success(fixture.repository.test_connection());
    assert!(
        connection
            .execute_batch("DROP TRIGGER suppress_yard_promotion")
            .is_ok()
    );
    drop(connection);
    let live = success(fixture.finalise(10));
    assert_eq!(live.deploy.status, YardDeployStatus::Live);
}

#[test]
fn finalisation_is_idempotent_and_missing_public_targets_fail_closed() {
    let fixture = FinaliseFixture::new("idempotent");
    let live = success(fixture.finalise(10));
    assert_eq!(live.deploy.status, YardDeployStatus::Live);
    assert_eq!(success(fixture.finalise(11)), live);
    assert_eq!(
        fixture
            .repository
            .yard_file_by_host(&fixture.yard.host_label, "missing.txt"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.rollback_web_yard(
            &fixture.yard.id,
            Some("deploy_missing"),
            12,
            &action(
                "yard.rolled_back",
                "yard_deploy",
                "yardId",
                &fixture.yard.id,
                12,
            ),
        ),
        Err(RepositoryError::NotFound)
    );
}
