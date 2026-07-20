use super::support::*;

fn version_for(
    repository: &SqliteRepository,
    deploy: &NewYardDeploy,
    number: u64,
    size: u64,
) -> NewYardFile {
    let version_id = format!("version_yard_cleanup_{number}");
    let object_path = format!("{}index.html", deploy.manifest_root);
    let storage_key = format!("objects/yard-cleanup/{number}");
    let connection = success(repository.test_connection());
    assert!(
        connection
            .execute(
                "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms, source) VALUES (?1, ?2, ?3, 1, ?4, 'complete', ?5, ?6, ?7, 'web')",
                rusqlite::params![
                    version_id,
                    deploy.project_id,
                    object_path,
                    storage_key,
                    size.cast_signed(),
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    number.cast_signed(),
                ],
            )
            .is_ok()
    );
    drop(connection);
    NewYardFile {
        normalized_path: "index.html".to_owned(),
        version_id,
        byte_size: size,
    }
}

fn version_exists(repository: &SqliteRepository, version_id: &str) -> bool {
    let connection = success(repository.test_connection());
    let count = success(connection.query_row(
        "SELECT COUNT(*) FROM object_versions WHERE id = ?1",
        [version_id],
        |row| row.get::<_, i64>(0),
    ));
    drop(connection);
    count == 1
}

#[test]
fn pruning_durably_plans_exact_release_bytes_before_metadata_finalization() {
    let (_temporary, repository, _fixture_version, size) = repository();
    let yard = yard("cleanup", 1);
    for number in 1..=11 {
        let deploy = deploy(&yard, number, false);
        assert!(start(&repository, &yard, &deploy).is_ok());
        let file = [version_for(&repository, &deploy, number, size)];
        assert!(
            repository
                .finalise_yard_deploy(
                    &deploy.id,
                    &file,
                    number,
                    &deployed(&deploy.id, 1, size, "live", number),
                )
                .is_ok()
        );
    }

    let cleanups = success(repository.pending_yard_cleanups(Some(&yard.id)));
    assert_eq!(cleanups.len(), 1);
    let cleanup = &cleanups[0];
    assert_eq!(cleanup.deploy_id, "deploy_cleanup_1");
    assert_eq!(cleanup.deletion.items.len(), 1);
    assert_eq!(
        cleanup.deletion.items[0].version_id,
        "version_yard_cleanup_1"
    );
    assert!(version_exists(&repository, "version_yard_cleanup_1"));

    let completed_at = 12;
    let mut completion = action(
        "yard.cleanup_completed",
        "yard_deploy",
        "deployId",
        &cleanup.deploy_id,
        completed_at,
    );
    completion.workspace_id.clone_from(&cleanup.workspace_id);
    completion.actor.clone_from(&cleanup.deletion.actor);
    completion
        .request_id
        .clone_from(&cleanup.deletion.request_id);
    assert!(
        repository
            .finish_deletion(&cleanup.deletion.id, completed_at, &completion)
            .is_ok()
    );
    assert!(success(repository.pending_yard_cleanups(Some(&yard.id))).is_empty());
    assert!(!version_exists(&repository, "version_yard_cleanup_1"));
}
