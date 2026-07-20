use super::support::*;

fn finalise(
    repository: &SqliteRepository,
    fixture: &FinaliseFixture,
    deploy: &NewYardDeploy,
    at: u64,
) {
    assert!(
        repository
            .finalise_yard_deploy(
                &deploy.id,
                &fixture.file,
                at,
                &deployed(&deploy.id, 1, fixture.size, "live", at),
            )
            .is_ok()
    );
}

#[test]
fn prune_failure_rolls_back_finalisation_and_stable_promotion() {
    let fixture = FinaliseFixture::new("prune-rollback");
    finalise(&fixture.repository, &fixture, &fixture.deploy, 1);
    let mut previous = fixture.deploy.id.clone();
    for number in 2..=10 {
        let next = deploy(&fixture.yard, number, false);
        assert!(start(&fixture.repository, &fixture.yard, &next).is_ok());
        finalise(&fixture.repository, &fixture, &next, number);
        previous = next.id;
    }
    let rejected = deploy(&fixture.yard, 11, false);
    assert!(start(&fixture.repository, &fixture.yard, &rejected).is_ok());
    let connection = success(fixture.repository.test_connection());
    assert!(
        connection
            .execute_batch(
                "CREATE TRIGGER reject_yard_prune BEFORE DELETE ON yard_deploy_files BEGIN SELECT RAISE(ABORT, 'reject prune'); END;",
            )
            .is_ok()
    );
    drop(connection);
    assert_eq!(
        fixture.repository.finalise_yard_deploy(
            &rejected.id,
            &fixture.file,
            11,
            &deployed(&rejected.id, 1, fixture.size, "live", 11),
        ),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        success(fixture.repository.web_yard_by_id(&fixture.yard.id)).current_deploy_id,
        Some(previous)
    );
    assert_eq!(
        success(fixture.repository.yard_deploy_by_id(&rejected.id)).status,
        YardDeployStatus::Uploading
    );
}
