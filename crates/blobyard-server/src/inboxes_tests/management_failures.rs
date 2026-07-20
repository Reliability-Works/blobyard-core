use super::*;
use crate::{
    Repository,
    repository_fault_tests::{Corruption, FaultingRepository},
};
use blobyard_api_client::{CreateInboxRequest, ListInboxesQuery};
use std::sync::Arc;

fn request() -> CreateInboxRequest {
    CreateInboxRequest {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
        name: "Inbox".to_owned(),
        expires: Some("1h".to_owned()),
    }
}

#[test]
fn create_preparation_rejects_invalid_contracts_and_repository_failures() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let principal = Principal(fixture.principal.clone());
    let request = request();

    let mut invalid_name = request.clone();
    invalid_name.name = "invalid\nname".to_owned();
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &invalid_name,
            Ok(1),
        )),
        StatusCode::BAD_REQUEST
    );
    let mut missing_workspace = request.clone();
    missing_workspace.workspace = "missing".parse().expect("workspace");
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &missing_workspace,
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );
    let mut invalid_expiry = request.clone();
    invalid_expiry.expires = Some("31d".to_owned());
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &invalid_expiry,
            Ok(1),
        )),
        StatusCode::BAD_REQUEST
    );
    let mut default_expiry = request.clone();
    default_expiry.expires = None;
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &default_expiry,
            Ok(u64::MAX - 7 * 24 * 60 * 60 * 1_000),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut failed = fixture.state;
    let inner: Arc<dyn Repository> = Arc::clone(&failed.repository);
    failed.repository = Arc::new(FaultingRepository::new(inner, 2));
    assert_eq!(
        error_status(operations::create_at(&failed, &principal, &request, Ok(1),)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn list_preparation_conceals_binding_repository_and_corrupt_summary_failures() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let principal = Principal(fixture.principal.clone());
    let query = ListInboxesQuery {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
        cursor: None,
    };

    let mut missing_workspace = query.clone();
    missing_workspace.workspace = "missing".parse().expect("workspace");
    assert_eq!(
        error_status(operations::list_at(
            &fixture.state,
            &principal,
            &missing_workspace,
        )),
        StatusCode::NOT_FOUND
    );
    let mut bound = principal.clone();
    bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::list_at(&fixture.state, &bound, &query)),
        StatusCode::NOT_FOUND
    );

    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut failed = fixture.state.clone();
    failed.repository = Arc::new(FaultingRepository::new(Arc::clone(&inner), 2));
    assert_eq!(
        error_status(operations::list_at(&failed, &principal, &query)),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let create = CreateInboxRequest {
        workspace: query.workspace.clone(),
        project: query.project.clone(),
        name: "Corrupt summary".to_owned(),
        expires: Some("1h".to_owned()),
    };
    let _ =
        operations::create_at(&fixture.state, &principal, &create, Ok(1)).expect("create inbox");
    let mut corrupt = fixture.state.clone();
    corrupt.repository = Arc::new(FaultingRepository::corrupting(
        inner,
        Corruption::InboxExpiry,
    ));
    assert_eq!(
        error_status(operations::list_at(&corrupt, &principal, &query)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
