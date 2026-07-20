use super::*;

fn revoke(
    fixture: &Fixture,
    preview_id: &str,
    workspace_id: &str,
    project_id: &str,
    revoked_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    fixture
        .repository
        .revoke_preview(preview_id, workspace_id, project_id, revoked_at_ms, event)
}

#[test]
fn revocation_rejects_invalid_and_foreign_identity() {
    let fixture = Fixture::new();
    fixture.create();
    let valid_event = event("preview.revoked", &fixture.preview, 1_100);
    for (preview_id, workspace_id, project_id) in [
        (
            "",
            fixture.preview.workspace_id.as_str(),
            fixture.preview.project_id.as_str(),
        ),
        (
            fixture.preview.id.as_str(),
            "",
            fixture.preview.project_id.as_str(),
        ),
        (
            fixture.preview.id.as_str(),
            fixture.preview.workspace_id.as_str(),
            "",
        ),
    ] {
        assert_eq!(
            revoke(
                &fixture,
                preview_id,
                workspace_id,
                project_id,
                1_100,
                &valid_event,
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    for (workspace_id, project_id) in [
        ("workspace_foreign", fixture.preview.project_id.as_str()),
        (fixture.preview.workspace_id.as_str(), "project_foreign"),
    ] {
        assert_eq!(
            revoke(
                &fixture,
                &fixture.preview.id,
                workspace_id,
                project_id,
                1_100,
                &valid_event,
            ),
            Err(RepositoryError::NotFound)
        );
    }
}

#[test]
fn revocation_rejects_invalid_audit_and_time() {
    let fixture = Fixture::new();
    fixture.create();
    assert_eq!(
        revoke(
            &fixture,
            &fixture.preview.id,
            &fixture.preview.workspace_id,
            &fixture.preview.project_id,
            1_100,
            &event("preview.wrong", &fixture.preview, 1_100),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        revoke(
            &fixture,
            &fixture.preview.id,
            &fixture.preview.workspace_id,
            &fixture.preview.project_id,
            u64::MAX,
            &event("preview.revoked", &fixture.preview, u64::MAX),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn revocation_detects_suppressed_updates() {
    let fixture = Fixture::new();
    fixture.create();
    let valid_event = event("preview.revoked", &fixture.preview, 1_100);
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER suppress_preview_revoke BEFORE UPDATE OF status ON previews
         WHEN NEW.status = 'revoked' BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("suppressing trigger");
    assert_eq!(
        revoke(
            &fixture,
            &fixture.preview.id,
            &fixture.preview.workspace_id,
            &fixture.preview.project_id,
            1_100,
            &valid_event,
        ),
        Err(RepositoryError::Conflict)
    );
}
