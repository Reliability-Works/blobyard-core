use super::support::*;

#[test]
fn skips_incomplete_and_already_pruned_records_without_a_current_deploy() {
    let (_temporary, repository, _version_id, _size) = repository();
    let history_yard = yard("history", 1);
    for number in 1..=12 {
        assert!(
            start(
                &repository,
                &history_yard,
                &deploy(&history_yard, number, true),
            )
            .is_ok()
        );
    }
    let mut connection = success(repository.test_connection());
    let transaction = success(connection.transaction());
    assert!(yard_history::prune(&transaction, &history_yard.id, None, 20).is_ok());
    assert!(
        transaction
            .execute(
                "UPDATE yard_deploys SET status = 'pruned', pruned_at_ms = 20 WHERE yard_id = ?1 AND id = (SELECT id FROM yard_deploys WHERE yard_id = ?1 ORDER BY id LIMIT 1)",
                [&history_yard.id],
            )
            .is_ok()
    );
    assert!(yard_history::prune_all(&transaction, &history_yard.id, 21).is_ok());
    assert!(transaction.commit().is_ok());
    drop(connection);
    assert!(
        success(repository.list_yard_deploys(&history_yard.id))
            .into_iter()
            .all(|record| record.status == YardDeployStatus::Pruned)
    );
}
