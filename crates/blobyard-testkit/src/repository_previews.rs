use blobyard_contract::{
    AuditValue, NewAuditEvent, NewPreview, NewPreviewFile, PreviewRepository, PreviewStatus,
    RepositoryError, TransferRepository,
};

/// Combined repository surface needed by preview conformance.
pub trait PreviewConformanceRepository: PreviewRepository + TransferRepository {}

impl<T: PreviewRepository + TransferRepository> PreviewConformanceRepository for T {}

/// Builds the canonical redacted preview audit fixture.
#[must_use]
pub fn preview_event(action: &str, preview_id: &str, now_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}_{now_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}_{now_ms}"),
        target_type: "preview".to_owned(),
        metadata: vec![(
            "previewId".to_owned(),
            AuditValue::String(preview_id.to_owned()),
        )],
        created_at_ms: now_ms,
    }
}

/// Runs deterministic preview snapshot, resolution, expiry, and revocation transitions.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn preview_conformance(
    repository: &dyn PreviewConformanceRepository,
) -> Result<(), RepositoryError> {
    if !repository.list_previews("project_fixture")?.is_empty() {
        return Err(RepositoryError::Unavailable);
    }
    let object = repository
        .list_stored_objects("project_fixture", Some("artifacts/build.zip"), false)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    let preview = NewPreview {
        id: "preview_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        capability_hash: "6".repeat(64),
        expires_at_ms: 8_000,
        created_at_ms: 7_000,
        files: vec![NewPreviewFile {
            normalized_path: "index.html".to_owned(),
            version_id: object.version.id.clone(),
        }],
    };
    let created = repository.create_preview(
        &preview,
        &preview_event("preview.created", &preview.id, preview.created_at_ms),
    )?;
    let target =
        repository.preview_file_by_capability(&preview.capability_hash, "index.html", 7_999)?;
    let listed = repository.list_previews(&preview.project_id)?;
    if created.status != PreviewStatus::Active
        || target.preview != created
        || target.normalized_path != "index.html"
        || target.object.version.id != object.version.id
        || listed != [created]
    {
        return Err(RepositoryError::Unavailable);
    }
    if repository.preview_file_by_capability(&preview.capability_hash, "missing", 7_999)
        != Err(RepositoryError::NotFound)
        || repository.preview_file_by_capability(&preview.capability_hash, "index.html", 8_000)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    let revoke_event = preview_event("preview.revoked", &preview.id, 7_500);
    if !repository.revoke_preview(
        &preview.id,
        &preview.workspace_id,
        &preview.project_id,
        7_500,
        &revoke_event,
    )? || repository.revoke_preview(
        &preview.id,
        &preview.workspace_id,
        &preview.project_id,
        7_500,
        &revoke_event,
    )? || repository.preview_file_by_capability(&preview.capability_hash, "index.html", 7_501)
        != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}
