#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{RenameRequest, rename_with_clock};
use crate::{error::ApiError, repository_fault_tests::FaultingRepository, transfers::test_seams};
use axum::response::IntoResponse;
use blobyard_contract::RepositoryError;
use blobyard_core::Slug;
use std::sync::Arc;

fn request(workspace: &str, name: &str) -> RenameRequest {
    RenameRequest {
        name: name.to_owned(),
        workspace: Slug::new(workspace).expect("workspace slug"),
    }
}

#[test]
fn rename_changes_the_namespace_and_records_the_previous_slug() {
    let fixture = test_seams::fixture(&["project:write"]);
    let response = rename_with_clock(
        &fixture.state,
        &crate::auth::Principal(fixture.principal.clone()),
        request("fixture", "Release Engineering"),
        Ok(42),
    )
    .expect("renamed");
    let value = serde_json::to_value(response.0).expect("response JSON");
    assert_eq!(value["data"]["id"], "workspace_fixture");
    assert_eq!(value["data"]["name"], "Release Engineering");
    assert_eq!(value["data"]["slug"], "release-engineering");
    assert_eq!(
        fixture
            .state
            .repository
            .workspace_by_slug(&Slug::new("fixture").expect("old slug")),
        Err(RepositoryError::NotFound)
    );
    let audit = fixture
        .state
        .repository
        .list_audit("workspace_fixture", None, 1)
        .expect("audit");
    assert_eq!(audit.items[0].action, "workspace.renamed");
    assert_eq!(
        audit.items[0].metadata,
        vec![(
            "previousSlug".to_owned(),
            blobyard_contract::AuditValue::String("fixture".to_owned()),
        )]
    );
}

#[test]
fn rename_fails_before_mutation_for_scope_ownership_clock_and_repository_errors() {
    let no_scope = test_seams::fixture(&["workspace:read"]);
    let foreign = test_seams::fixture(&["project:write"]);
    let _foreign_upload = foreign.seed_foreign_upload();
    let valid = test_seams::fixture(&["project:write"]);
    let mut faulting_state = valid.state.clone();
    faulting_state.repository = Arc::new(FaultingRepository::new(
        Arc::clone(&valid.state.repository),
        1,
    ));
    let mut lookup_faulting_state = valid.state.clone();
    lookup_faulting_state.repository = Arc::new(FaultingRepository::new(
        Arc::clone(&valid.state.repository),
        0,
    ));
    for result in [
        rename_with_clock(
            &no_scope.state,
            &crate::auth::Principal(no_scope.principal.clone()),
            request("fixture", "Renamed"),
            Ok(1),
        ),
        rename_with_clock(
            &foreign.state,
            &crate::auth::Principal(foreign.principal.clone()),
            request("foreign", "Renamed"),
            Ok(1),
        ),
        rename_with_clock(
            &valid.state,
            &crate::auth::Principal(valid.principal.clone()),
            request("fixture", "Renamed"),
            Err(ApiError::internal()),
        ),
        rename_with_clock(
            &faulting_state,
            &crate::auth::Principal(valid.principal.clone()),
            request("fixture", "Renamed"),
            Ok(1),
        ),
        rename_with_clock(
            &lookup_faulting_state,
            &crate::auth::Principal(valid.principal.clone()),
            request("fixture", "Renamed"),
            Ok(1),
        ),
    ] {
        assert!(result.is_err());
    }
    for fixture in [&no_scope, &foreign, &valid] {
        assert!(
            fixture
                .state
                .repository
                .workspace_by_slug(&Slug::new("fixture").expect("workspace"))
                .is_ok()
        );
    }
}

#[test]
fn invalid_rename_names_fail_before_repository_access() {
    let fixture = test_seams::fixture(&["project:write"]);
    for name in ["", "---", "line\nbreak"] {
        let error = rename_with_clock(
            &fixture.state,
            &crate::auth::Principal(fixture.principal.clone()),
            request("fixture", name),
            Ok(1),
        )
        .err()
        .expect("invalid rename");
        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }
}
