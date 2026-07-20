use blobyard_contract::{
    AuditValue, InboxRateResult, InboxRepository, InboxStatus, NewAuditEvent, NewInbox,
    NewInboxUpload, ObjectSource, RepositoryError, TransferRepository, UploadState,
};

/// Repository surface required by the portable inbox conformance journey.
pub trait InboxConformanceRepository: InboxRepository + TransferRepository {}

impl<T> InboxConformanceRepository for T where T: InboxRepository + TransferRepository + ?Sized {}

/// Runs deterministic public-inbox transitions against a populated adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn inbox_conformance(
    repository: &dyn InboxConformanceRepository,
) -> Result<(), RepositoryError> {
    let inbox = create_and_resolve(repository)?;
    exercise_rate_limit(repository)?;
    exercise_complete(repository, &inbox)?;
    exercise_abort_and_capacity(repository, &inbox)?;
    exercise_expiry_and_revocation(repository, &inbox)
}

fn create_and_resolve(
    repository: &dyn InboxConformanceRepository,
) -> Result<NewInbox, RepositoryError> {
    let inbox = NewInbox {
        id: "inbox_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: "Fixture inbox".to_owned(),
        capability_hash: super::hash('1'),
        expires_at_ms: 5_000,
        maximum_files: 2,
        maximum_bytes: 10,
        created_at_ms: 1_000,
    };
    let created =
        repository.create_inbox(&inbox, &inbox_event("inbox.created", &inbox.id, 1_000))?;
    let listed = repository.list_inboxes(&inbox.project_id)?;
    let resolved = repository.inbox_by_capability(&inbox.capability_hash, 1_001)?;
    if created.status != InboxStatus::Active
        || created.current_files != 0
        || created.current_bytes != 0
        || listed != [created]
        || resolved.id != inbox.id
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(inbox)
}

fn exercise_rate_limit(repository: &dyn InboxConformanceRepository) -> Result<(), RepositoryError> {
    let key = super::hash('2');
    if repository.consume_inbox_rate(&key, 1_000, 2, 1_000)? != InboxRateResult::Allowed
        || repository.consume_inbox_rate(&key, 1_000, 2, 1_001)? != InboxRateResult::Allowed
        || repository.consume_inbox_rate(&key, 1_000, 2, 1_500)?
            != (InboxRateResult::Limited {
                retry_after_seconds: 1,
            })
        || repository.consume_inbox_rate(&key, 1_000, 2, 2_000)? != InboxRateResult::Allowed
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn exercise_complete(
    repository: &dyn InboxConformanceRepository,
    inbox: &NewInbox,
) -> Result<(), RepositoryError> {
    let reservation = inbox_upload("inbox_upload_complete", 1_100);
    let principal = inbox_principal(inbox, 1_100, '3');
    let reserved = repository.reserve_inbox_upload(&principal, &reservation)?;
    let counters = repository
        .list_inboxes(&inbox.project_id)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    if reserved.version.source != ObjectSource::Inbox
        || counters.reserved_files != 1
        || counters.reserved_bytes != 5
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.record_uploaded_bytes(&reservation.id, 5, super::hello_checksum())?;
    let completed = repository.complete_inbox_upload(
        &inbox.capability_hash,
        &reservation.id,
        1_101,
        &inbox_upload_event(&inbox.id, 1_101),
    )?;
    let counters = repository
        .list_inboxes(&inbox.project_id)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    if completed.state != UploadState::Complete
        || counters.current_files != 1
        || counters.current_bytes != 5
        || counters.reserved_files != 0
        || counters.reserved_bytes != 0
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn exercise_abort_and_capacity(
    repository: &dyn InboxConformanceRepository,
    inbox: &NewInbox,
) -> Result<(), RepositoryError> {
    let reservation = inbox_upload("inbox_upload_abort", 1_200);
    let principal = inbox_principal(inbox, 1_200, '4');
    repository.reserve_inbox_upload(&principal, &reservation)?;
    let prior = repository.abort_inbox_upload(&inbox.capability_hash, &reservation.id, 1_201)?;
    let current = reservation_record(repository, inbox, &reservation.id, 1_201)?;
    if prior.state != blobyard_contract::ReservationState::Requested
        || current.state != blobyard_contract::ReservationState::Aborted
    {
        return Err(RepositoryError::Unavailable);
    }
    let mut oversized = inbox_upload("inbox_upload_over_capacity", 1_300);
    oversized.expected_size = 6;
    if repository.reserve_inbox_upload(&inbox_principal(inbox, 1_300, '5'), &oversized)
        != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    let counters = repository
        .list_inboxes(&inbox.project_id)?
        .pop()
        .ok_or(RepositoryError::Unavailable)?;
    if counters.reserved_files != 0 || counters.reserved_bytes != 0 {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn reservation_record(
    repository: &dyn InboxConformanceRepository,
    inbox: &NewInbox,
    upload_id: &str,
    now_ms: u64,
) -> Result<blobyard_contract::UploadReservationRecord, RepositoryError> {
    repository.inbox_upload_by_id(&inbox.capability_hash, upload_id, now_ms)
}

fn exercise_expiry_and_revocation(
    repository: &dyn InboxConformanceRepository,
    inbox: &NewInbox,
) -> Result<(), RepositoryError> {
    if repository.inbox_by_capability(&inbox.capability_hash, inbox.expires_at_ms)
        != Err(RepositoryError::NotFound)
        || !repository.revoke_inbox(
            &inbox.id,
            &inbox.workspace_id,
            1_400,
            &inbox_event("inbox.revoked", &inbox.id, 1_400),
        )?
        || repository.revoke_inbox(
            &inbox.id,
            &inbox.workspace_id,
            1_401,
            &inbox_event("inbox.revoked", &inbox.id, 1_401),
        )?
        || repository.inbox_by_capability(&inbox.capability_hash, 1_401)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn inbox_upload(id: &str, created_at_ms: u64) -> blobyard_contract::NewUploadReservation {
    let mut upload = super::upload(id, "project_fixture", "inbox/build.zip", '6');
    let suffix = match id {
        "inbox_upload_complete" => '1',
        "inbox_upload_abort" => '2',
        _other => '3',
    };
    upload.capability_hash = format!("{}{}", "0".repeat(63), suffix);
    upload.created_at_ms = created_at_ms;
    upload.expires_at_ms = 4_000;
    upload.source = ObjectSource::Inbox;
    upload.git_repository = None;
    upload.git_commit = None;
    upload.git_branch = None;
    upload
}

fn inbox_principal(inbox: &NewInbox, now_ms: u64, fingerprint: char) -> NewInboxUpload {
    NewInboxUpload {
        capability_hash: inbox.capability_hash.clone(),
        fingerprint_hash: super::hash(fingerprint),
        now_ms,
    }
}

/// Builds the canonical non-secret inbox management audit fixture.
#[must_use]
pub fn inbox_event(action: &str, inbox_id: &str, created_at_ms: u64) -> NewAuditEvent {
    super::events::capability_event(action, "inbox", "inboxId", inbox_id, created_at_ms)
}

/// Builds the canonical non-secret inbox upload audit fixture.
#[must_use]
pub fn inbox_upload_event(inbox_id: &str, created_at_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_inbox_upload_{created_at_ms}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: inbox_id.to_owned(),
        action: "inbox.uploaded".to_owned(),
        request_id: format!("request_{created_at_ms}"),
        target_type: "object_version".to_owned(),
        metadata: vec![
            ("byteSize".to_owned(), AuditValue::Number(5)),
            ("source".to_owned(), AuditValue::String("inbox".to_owned())),
        ],
        created_at_ms,
    }
}
