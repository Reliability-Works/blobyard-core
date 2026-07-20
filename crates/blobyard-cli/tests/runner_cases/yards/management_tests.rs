use super::super::support::{Fixture, ok, result_json};
use super::{deploy, deployment_response, yard};
use blobyard_api_client::Endpoint;
use blobyard_core::{BlobyardError, ErrorCode};

#[tokio::test]
async fn yard_list_uses_the_typed_project_route() {
    let list = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "list",
        ],
        vec![ok(
            serde_json::json!({
                "items": [yard("documentation", Some("deploy_1")), yard("app-site", None)],
                "nextCursor": null
            }),
            "req_1",
        )],
        Some("ci-token"),
        None,
    );
    let json = result_json(list.runner.execute(&list.command).await.expect("list"));
    assert_eq!(json["data"]["yards"].as_array().expect("yards").len(), 2);
    assert_eq!(
        list.transport.requests()[0].query().expect("query"),
        "workspace=team&project=web"
    );
}

#[tokio::test]
async fn yard_show_selects_the_only_yard() {
    let show = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "show",
        ],
        vec![ok(
            serde_json::json!({ "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null }),
            "req_show",
        )],
        Some("ci-token"),
        None,
    );
    let shown = result_json(show.runner.execute(&show.command).await.expect("show"));
    assert_eq!(shown["data"]["name"], "documentation");
}

#[tokio::test]
async fn yard_history_resolves_the_name_then_lists_immutable_deploys() {
    let history = Fixture::new(
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
            ok(
                serde_json::json!({ "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null }),
                "req_yard",
            ),
            ok(
                serde_json::json!({ "items": [deploy("deploy_1", "live", true)], "nextCursor": null }),
                "req_history",
            ),
        ],
        Some("ci-token"),
        None,
    );
    let history_json = result_json(
        history
            .runner
            .execute(&history.command)
            .await
            .expect("history"),
    );
    assert_eq!(history_json["data"]["deploys"][0]["isCurrent"], true);
    assert_eq!(
        history.transport.requests()[1].query().expect("query"),
        "yardId=yard_documentation"
    );
}

#[tokio::test]
async fn yard_rollback_resolves_the_name_and_sends_the_selected_deploy() {
    let rollback = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "rollback",
            "documentation",
            "deploy_1",
        ],
        vec![
            ok(
                serde_json::json!({ "items": [yard("documentation", Some("deploy_2"))], "nextCursor": null }),
                "req_yard",
            ),
            deployment_response("documentation", "deploy_1", "live"),
        ],
        Some("ci-token"),
        None,
    );
    rollback
        .runner
        .execute(&rollback.command)
        .await
        .expect("rollback");
    assert_eq!(
        rollback.transport.requests()[1].endpoint(),
        Endpoint::RollbackWebYard
    );
    assert_eq!(
        rollback.transport.requests()[1]
            .body()
            .expect("rollback body"),
        &serde_json::json!({ "yardId": "yard_documentation", "deployId": "deploy_1" })
    );
}

#[tokio::test]
async fn forced_yard_delete_resolves_the_name_and_uses_the_typed_route() {
    let delete = Fixture::new(
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
            ok(
                serde_json::json!({ "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null }),
                "req_yard",
            ),
            ok(serde_json::json!({}), "req_delete"),
        ],
        Some("ci-token"),
        None,
    );
    delete
        .runner
        .execute(&delete.command)
        .await
        .expect("delete");
    let request = &delete.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::DeleteWebYard);
    assert_eq!(
        request.body().expect("delete body"),
        &serde_json::json!({ "yardId": "yard_documentation" })
    );
}

#[tokio::test]
async fn destructive_delete_requires_force_without_an_interactive_terminal() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "delete",
            "documentation",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("force");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
    assert!(error.message().contains("--force"));
    assert!(fixture.transport.requests().is_empty());
}

#[tokio::test]
async fn interactive_delete_accepts_explicit_confirmation() {
    let mut fixture = delete_fixture(vec![
        ok(
            serde_json::json!({
                "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null
            }),
            "req_yard",
        ),
        ok(serde_json::json!({}), "req_delete"),
    ]);
    fixture.runner.set_test_confirmation(true, Ok(true));
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("delete");
    assert_eq!(
        fixture.transport.requests()[1].endpoint(),
        Endpoint::DeleteWebYard
    );
}

#[tokio::test]
async fn interactive_delete_preserves_decline_and_confirmation_errors() {
    let mut declined = delete_fixture(Vec::new());
    declined.runner.set_test_confirmation(true, Ok(false));
    assert_eq!(
        declined
            .runner
            .execute(&declined.command)
            .await
            .expect_err("declined")
            .code(),
        ErrorCode::Interrupted
    );

    let mut failed = delete_fixture(Vec::new());
    failed.runner.set_test_confirmation(
        true,
        Err(BlobyardError::from_code(ErrorCode::InternalError)),
    );
    assert_eq!(
        failed
            .runner
            .execute(&failed.command)
            .await
            .expect_err("confirmation failure")
            .code(),
        ErrorCode::InternalError
    );
}

fn delete_fixture(responses: Vec<blobyard_api_client::RawResponse>) -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "yard",
            "delete",
            "documentation",
        ],
        responses,
        Some("ci-token"),
        None,
    )
}
