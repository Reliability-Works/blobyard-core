use super::super::support::{Fixture, api_failure, result_json, signed_server};
use super::super::transfer_fixtures::{completion, empty_reply, reservation};
use super::{
    SHA256, deployment_response, failed_deploy_requests, failed_deploy_response,
    public_deploy_fixture, start_response,
};
use blobyard_api_client::{ApiRequest, Endpoint};
use blobyard_core::ErrorCode;

#[tokio::test]
async fn deploy_starts_uploads_and_finalises_with_one_client_identifier() {
    let root = tempfile::tempdir().expect("root");
    let site = root.path().join("site");
    std::fs::create_dir(&site).expect("site");
    std::fs::write(site.join("index.html"), b"abc").expect("index");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &site.to_string_lossy(),
            "--yard",
            "documentation",
            "--spa",
            "--public",
        ],
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            reservation("single", &url, None, "upload_yard"),
            completion(3, SHA256),
            deployment_response("documentation", "deploy_started", "live"),
        ],
        Some("ci-token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("deploy");
    let json = result_json(result);
    assert_eq!(json["data"]["yard"], "documentation");
    assert_eq!(
        json["data"]["yardUrl"],
        "https://documentation-123456789-team.blobyard.app"
    );
    assert_eq!(
        json["data"]["deploymentUrl"],
        "https://documentation-0123456789-team.blobyard.app"
    );
    assert_eq!(json["data"]["deployId"], "deploy_started");
    assert_eq!(json["data"]["status"], "live");

    assert_deploy_request_contract(&fixture.transport.requests());
    assert_eq!(storage.await.expect("storage").len(), 1);
}

fn assert_deploy_request_contract(requests: &[ApiRequest]) {
    assert_eq!(
        requests
            .iter()
            .map(blobyard_api_client::ApiRequest::endpoint)
            .collect::<Vec<_>>(),
        [
            Endpoint::StartYardDeploy,
            Endpoint::RequestUpload,
            Endpoint::CompleteUpload,
            Endpoint::FinaliseYardDeploy,
        ]
    );
    let client_id = requests[0].body().expect("start body")["clientDeployId"]
        .as_str()
        .expect("client id");
    assert_eq!(client_id.len(), 32);
    assert_eq!(requests[0].body().expect("start body")["public"], true);
    assert_eq!(requests[0].body().expect("start body")["spa"], true);
    assert_eq!(
        requests[3].body().expect("finalise body")["deployId"],
        "deploy_started"
    );
    assert_eq!(requests[0].idempotency_key(), None);
    assert_eq!(requests[3].idempotency_key(), None);
    assert_eq!(
        requests[1].body().expect("upload body")["path"],
        ".blobyard-yard/yard_documentation/reserved/index.html"
    );
}

#[tokio::test]
async fn deploy_requires_root_index_and_noninteractive_public_acknowledgement() {
    let root = tempfile::tempdir().expect("root");
    let missing_index = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &root.path().to_string_lossy(),
            "--yard",
            "documentation",
            "--public",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let error = missing_index
        .runner
        .execute(&missing_index.command)
        .await
        .expect_err("index");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
    assert!(error.message().contains("index.html"));
    assert!(missing_index.transport.requests().is_empty());

    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let no_public = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &root.path().to_string_lossy(),
            "--yard",
            "documentation",
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let error = no_public
        .runner
        .execute(&no_public.command)
        .await
        .expect_err("public");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
    assert!(error.message().contains("--public"));
    assert!(no_public.transport.requests().is_empty());
}

#[tokio::test]
async fn deploy_marks_started_work_failed_without_hiding_the_upload_error() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let fixture = public_deploy_fixture(
        root.path(),
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            api_failure(ErrorCode::PlanLimit, "req_reserve"),
            failed_deploy_response(),
        ],
    );
    let requests = failed_deploy_requests(&fixture, ErrorCode::PlanLimit, 3).await;
    assert_eq!(
        requests[2].body().expect("fail")["failureCode"],
        "PLAN_LIMIT"
    );
}

#[tokio::test]
async fn deploy_fails_inconsistent_start_metadata_and_cleans_up_the_reservation() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let fixture = public_deploy_fixture(
        root.path(),
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/wrong-yard/reserved/",
            ),
            failed_deploy_response(),
        ],
    );
    failed_deploy_requests(&fixture, ErrorCode::ProviderUnavailable, 2).await;
}

#[tokio::test]
async fn deploy_all_preserves_success_and_reports_each_local_failure() {
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let fixture = Fixture::with_project_config(
        &["blobyard", "deploy", "--all", "--public"],
        vec![
            start_response(
                "dashboard",
                "deploy_started",
                ".blobyard-yard/yard_dashboard/reserved/",
            ),
            reservation("single", &url, None, "upload_yard"),
            completion(3, SHA256),
            deployment_response("dashboard", "deploy_started", "live"),
        ],
        Some("ci-token"),
        None,
        concat!(
            "workspace = \"team\"\nproject = \"web\"\n",
            "[yards.dashboard]\ndirectory = \"dashboard\"\n",
            "[yards.marketing]\ndirectory = \"marketing\"\n",
        ),
    );
    std::fs::create_dir(fixture.temp.path().join("dashboard")).expect("dashboard");
    std::fs::write(fixture.temp.path().join("dashboard/index.html"), b"abc").expect("index");
    std::fs::create_dir(fixture.temp.path().join("marketing")).expect("marketing");
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("batch result");
    let json = result_json(result);
    assert_eq!(json["ok"], false);
    assert_eq!(
        json["data"]["results"].as_array().expect("results").len(),
        2
    );
    assert_eq!(json["data"]["results"][0]["yard"], "dashboard");
    assert_eq!(json["data"]["results"][0]["ok"], true);
    assert_eq!(json["data"]["results"][1]["yard"], "marketing");
    assert_eq!(json["data"]["results"][1]["ok"], false);
    assert_eq!(json["error"]["code"], "INVALID_REQUEST");
    assert_eq!(storage.await.expect("storage").len(), 1);
}
