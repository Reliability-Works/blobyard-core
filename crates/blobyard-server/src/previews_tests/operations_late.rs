use super::*;

#[tokio::test]
async fn list_fails_closed_for_corrupt_records() {
    let (fixture, principal, _preview) = created_preview_fixture().await;
    for corruption in [Corruption::PreviewCreatedAt, Corruption::PreviewExpiresAt] {
        let mut corrupt = fixture.state.clone();
        let inner: Arc<dyn Repository> = Arc::clone(&corrupt.repository);
        corrupt.repository = Arc::new(FaultingRepository::corrupting(inner, corruption));
        assert_eq!(
            error_status(operations::list_at(
                &corrupt,
                &principal,
                &list_query(),
                Ok(1_001),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn revoke_fails_closed_for_authorization_clock_and_repository_errors() {
    let (fixture, principal, preview) = created_preview_fixture().await;
    let request = RevokePreviewRequest {
        preview_id: preview.id,
    };
    let mut project_bound = principal.clone();
    project_bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::revoke_at(
            &fixture.state,
            &project_bound,
            &request,
            Ok(1_001),
        )),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(operations::revoke_at(
            &fixture.state,
            &principal,
            &request,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let failed = faulted_state(fixture.state, 1);
    assert_eq!(
        error_status(operations::revoke_at(
            &failed,
            &principal,
            &request,
            Ok(1_001),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
