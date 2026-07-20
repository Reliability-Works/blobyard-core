use super::support::*;

#[test]
fn rejects_suspended_names_mismatched_replays_and_missing_projects() {
    let (_temporary, repository, _version_id, _size) = repository();
    let first_yard = yard("docs", 1);
    let first_deploy = deploy(&first_yard, 1, true);
    assert!(start(&repository, &first_yard, &first_deploy).is_ok());
    suspend_yard(&repository, &first_yard.id);
    assert_eq!(
        start(&repository, &first_yard, &deploy(&first_yard, 2, true)),
        Err(RepositoryError::Conflict)
    );
    let mut replay = first_deploy;
    replay.spa = false;
    assert_eq!(
        start(&repository, &first_yard, &replay),
        Err(RepositoryError::Conflict)
    );
    let mut missing_yard = yard("missing", 3);
    missing_yard.project_id = "project_missing".to_owned();
    let missing_deploy = deploy(&missing_yard, 3, true);
    assert_eq!(
        start(&repository, &missing_yard, &missing_deploy),
        Err(RepositoryError::NotFound)
    );
}
