use super::super::support::{Fixture, result_json, signed_server};
use super::super::transfer_fixtures::{completion, empty_reply, reservation};
use super::{
    SHA256, deployment_response, failed_deploy_requests, failed_deploy_response,
    public_deploy_fixture, start_response,
};
use blobyard_core::ErrorCode;

#[tokio::test]
async fn deploy_rejects_a_mismatched_finalise_response_and_marks_the_deploy_failed() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let fixture = public_deploy_fixture(
        root.path(),
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            reservation("single", &url, None, "upload_yard"),
            completion(3, SHA256),
            deployment_response("documentation", "wrong_deploy", "live"),
            failed_deploy_response(),
        ],
    );
    failed_deploy_requests(&fixture, ErrorCode::ProviderUnavailable, 5).await;
    assert_eq!(storage.await.expect("storage").len(), 1);
}

#[tokio::test]
async fn deploy_all_returns_a_successful_result_when_every_yard_is_published() {
    let (url, storage) = signed_server(vec![empty_reply("200 OK"), empty_reply("200 OK")]).await;
    let fixture = Fixture::with_project_config(
        &["blobyard", "deploy", "--all", "--public"],
        vec![
            start_response(
                "dashboard",
                "deploy_dashboard",
                ".blobyard-yard/yard_dashboard/reserved/",
            ),
            reservation("single", &url, None, "upload_dashboard"),
            completion(3, SHA256),
            deployment_response("dashboard", "deploy_dashboard", "live"),
            start_response(
                "marketing",
                "deploy_marketing",
                ".blobyard-yard/yard_marketing/reserved/",
            ),
            reservation("single", &url, None, "upload_marketing"),
            completion(3, SHA256),
            deployment_response("marketing", "deploy_marketing", "live"),
        ],
        Some("ci-token"),
        None,
        concat!(
            "workspace = \"team\"\nproject = \"web\"\n",
            "[yards.dashboard]\ndirectory = \"dashboard\"\n",
            "[yards.marketing]\ndirectory = \"marketing\"\n",
        ),
    );
    for name in ["dashboard", "marketing"] {
        let directory = fixture.temp.path().join(name);
        std::fs::create_dir(&directory).expect("directory");
        std::fs::write(directory.join("index.html"), b"abc").expect("index");
    }
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("batch");
    let json = result_json(result);
    assert_eq!(json["ok"], true);
    assert_eq!(
        json["data"]["results"].as_array().expect("results").len(),
        2
    );
    assert!(
        json["data"]["results"]
            .as_array()
            .expect("results")
            .iter()
            .all(|item| item["ok"] == true)
    );
    assert_eq!(storage.await.expect("storage").len(), 2);
}
