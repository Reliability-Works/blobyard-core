use super::{
    success,
    transaction_edges::support::{created, deploy, repository, yard},
};
use crate::adapter::SqliteRepository;
use blobyard_contract::{
    NewWebYard, WebYardRepository, YardEnvironmentKind, YardEnvironmentStatus,
};

fn created_yard() -> (tempfile::TempDir, SqliteRepository, NewWebYard) {
    let (temporary, repository, _version, _size) = repository();
    let candidate = yard("docs", 1);
    success(repository.start_yard_deploy(
        &candidate,
        &deploy(&candidate, 1, false),
        &created(&candidate.id, 1),
    ));
    (temporary, repository, candidate)
}

#[test]
fn created_yards_backfill_one_deterministic_production_environment() {
    let (_temporary, repository, candidate) = created_yard();
    let environments = success(repository.list_yard_environments(&candidate.id));
    assert_eq!(environments.len(), 1);
    assert_eq!(environments[0].id, format!("yardenv_{}", candidate.id));
    assert_eq!(environments[0].yard_id, candidate.id);
    assert_eq!(environments[0].name.as_str(), "production");
    assert_eq!(environments[0].kind, YardEnvironmentKind::Production);
    assert_eq!(environments[0].status, YardEnvironmentStatus::Active);
    assert_eq!(environments[0].created_at_ms, 1);
    assert_eq!(environments[0].updated_at_ms, 1);
    assert!(
        success(repository.list_yard_environments("yard_unknown")).is_empty(),
        "unknown Yards must produce an empty environment list"
    );
}

#[test]
fn environment_lists_order_production_first_and_exclude_deleted_rows() {
    let (_temporary, repository, candidate) = created_yard();
    let connection = success(repository.test_connection());
    success(connection.execute_batch(&format!(
        "INSERT INTO yard_environments VALUES ('yardenv_staging', '{id}', 'staging', 'staging', 'active', 2, 2, NULL);
         INSERT INTO yard_environments VALUES ('yardenv_preview', '{id}', 'preview', 'preview', 'active', 3, 3, NULL);
         INSERT INTO yard_environments VALUES ('yardenv_removed', '{id}', 'removed', 'preview', 'deleted', 4, 5, 5);",
        id = candidate.id
    )));
    drop(connection);
    let environments = success(repository.list_yard_environments(&candidate.id));
    let names = environments
        .iter()
        .map(|environment| environment.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["production", "preview", "staging"]);
}
