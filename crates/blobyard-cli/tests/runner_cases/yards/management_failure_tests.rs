use super::super::support::{Fixture, api_failure, ok};
use super::yard;
use crate::commands::{Command, RollbackYardArgs, YardCommand};
use blobyard_core::ErrorCode;

fn fixture(args: &[&str], responses: Vec<blobyard_api_client::RawResponse>) -> Fixture {
    Fixture::new(args, responses, Some("ci-token"), None)
}

fn yard_page(
    items: &serde_json::Value,
    cursor: &serde_json::Value,
) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({ "items": items, "nextCursor": cursor }),
        "req_yards",
    )
}

#[tokio::test]
async fn yard_list_propagates_scope_api_and_pagination_failures() {
    let missing_scope = fixture(&["blobyard", "yard", "list"], Vec::new());
    assert!(
        missing_scope
            .runner
            .execute(&missing_scope.command)
            .await
            .is_err()
    );

    let remote = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "list",
        ],
        vec![api_failure(ErrorCode::ProviderUnavailable, "req_list")],
    );
    assert!(remote.runner.execute(&remote.command).await.is_err());

    let cursor = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "list",
        ],
        vec![yard_page(
            &serde_json::json!([]),
            &serde_json::json!("next"),
        )],
    );
    assert!(cursor.runner.execute(&cursor.command).await.is_err());
}

#[tokio::test]
async fn yard_show_propagates_list_and_selection_failures() {
    let missing_scope = fixture(&["blobyard", "yard", "show"], Vec::new());
    assert!(
        missing_scope
            .runner
            .execute(&missing_scope.command)
            .await
            .is_err()
    );

    let empty = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "show",
        ],
        vec![yard_page(&serde_json::json!([]), &serde_json::Value::Null)],
    );
    assert!(empty.runner.execute(&empty.command).await.is_err());
}

#[tokio::test]
async fn yard_history_rejects_names_and_missing_yards_before_history_access() {
    let invalid = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "history",
            "api",
        ],
        Vec::new(),
    );
    assert!(invalid.runner.execute(&invalid.command).await.is_err());

    let missing_scope = fixture(
        &["blobyard", "yard", "history", "documentation"],
        Vec::new(),
    );
    assert!(
        missing_scope
            .runner
            .execute(&missing_scope.command)
            .await
            .is_err()
    );

    let missing = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "history",
            "documentation",
        ],
        vec![yard_page(&serde_json::json!([]), &serde_json::Value::Null)],
    );
    assert!(missing.runner.execute(&missing.command).await.is_err());
}

#[tokio::test]
async fn yard_history_propagates_remote_and_pagination_failures() {
    for response in [
        api_failure(ErrorCode::ProviderUnavailable, "req_history"),
        ok(
            serde_json::json!({ "items": [], "nextCursor": "next" }),
            "req_history",
        ),
    ] {
        let history = fixture(
            &[
                "blobyard",
                "--workspace",
                "team",
                "--project",
                "web",
                "yard",
                "history",
                "documentation",
            ],
            vec![
                yard_page(
                    &serde_json::json!([yard("documentation", Some("deploy_1"))]),
                    &serde_json::Value::Null,
                ),
                response,
            ],
        );
        assert!(history.runner.execute(&history.command).await.is_err());
    }
}

#[tokio::test]
async fn yard_rollback_rejects_invalid_names_and_deploy_ids() {
    let invalid = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "rollback",
            "api",
        ],
        Vec::new(),
    );
    assert!(invalid.runner.execute(&invalid.command).await.is_err());

    let mut empty_id = fixture(&["blobyard", "yard", "list"], Vec::new());
    empty_id.command = Command::Yard {
        command: YardCommand::Rollback(RollbackYardArgs {
            name: "documentation".into(),
            deploy_id: Some(String::new()),
        }),
    };
    assert!(empty_id.runner.execute(&empty_id.command).await.is_err());
}

#[tokio::test]
async fn yard_rollback_propagates_lookup_and_mutation_failures() {
    rollback_failure(
        &["blobyard", "yard", "rollback", "documentation"],
        Vec::new(),
    )
    .await;
    rollback_failure(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "rollback",
            "documentation",
        ],
        vec![yard_page(&serde_json::json!([]), &serde_json::Value::Null)],
    )
    .await;
    rollback_failure(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "rollback",
            "documentation",
        ],
        vec![
            yard_page(
                &serde_json::json!([yard("documentation", Some("deploy_1"))]),
                &serde_json::Value::Null,
            ),
            api_failure(ErrorCode::ProviderUnavailable, "req_rollback"),
        ],
    )
    .await;
}

async fn rollback_failure(args: &[&str], responses: Vec<blobyard_api_client::RawResponse>) {
    let rollback = fixture(args, responses);
    assert!(rollback.runner.execute(&rollback.command).await.is_err());
}

#[tokio::test]
async fn yard_delete_propagates_validation_lookup_and_mutation_failures() {
    let invalid = fixture(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "delete",
            "api",
            "--force",
        ],
        Vec::new(),
    );
    assert!(invalid.runner.execute(&invalid.command).await.is_err());

    delete_failure(
        &["blobyard", "yard", "delete", "documentation", "--force"],
        Vec::new(),
    )
    .await;
    delete_failure(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "delete",
            "documentation",
            "--force",
        ],
        vec![yard_page(&serde_json::json!([]), &serde_json::Value::Null)],
    )
    .await;
    delete_failure(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "delete",
            "documentation",
            "--force",
        ],
        vec![
            yard_page(
                &serde_json::json!([yard("documentation", Some("deploy_1"))]),
                &serde_json::Value::Null,
            ),
            api_failure(ErrorCode::ProviderUnavailable, "req_delete"),
        ],
    )
    .await;
}

async fn delete_failure(args: &[&str], responses: Vec<blobyard_api_client::RawResponse>) {
    let delete = fixture(args, responses);
    assert!(delete.runner.execute(&delete.command).await.is_err());
}
