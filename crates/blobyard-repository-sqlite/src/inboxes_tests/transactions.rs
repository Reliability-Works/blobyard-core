use super::*;
use blobyard_contract::{
    AuditValue, InboxRepository, LifecycleRepository, NewUploadPartGrant, ReservationStrategy,
    TransferRepository, UploadState,
};

fn reserve(fixture: &Fixture, id: &str, created_at_ms: u64) -> NewUploadReservation {
    let reservation = upload(id, created_at_ms);
    fixture
        .repository
        .reserve_inbox_upload(&principal(&fixture.inbox, created_at_ms), &reservation)
        .expect("inbox reservation");
    reservation
}

fn states(fixture: &Fixture, upload_id: &str) -> (String, String, String) {
    let connection = fixture.repository.test_connection().expect("connection");
    let reservation = connection
        .query_row(
            "SELECT state FROM upload_reservations WHERE id = ?1",
            [upload_id],
            |row| row.get(0),
        )
        .expect("reservation state");
    let version = connection
        .query_row(
            "SELECT v.state FROM object_versions v JOIN upload_reservations r ON r.version_id = v.id WHERE r.id = ?1",
            [upload_id],
            |row| row.get(0),
        )
        .expect("version state");
    let inbox = connection
        .query_row(
            "SELECT status FROM inbox_uploads WHERE upload_id = ?1",
            [upload_id],
            |row| row.get(0),
        )
        .expect("inbox upload state");
    drop(connection);
    (reservation, version, inbox)
}

fn clear_reserved_capacity(fixture: &Fixture) {
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE inboxes SET reserved_files = 0, reserved_bytes = 0 WHERE id = ?1",
            [&fixture.inbox.id],
        )
        .expect("clear reserved counters");
}

#[test]
fn capacity_failures_roll_back_completion_and_abort_mutations() {
    let fixture = Fixture::new();
    fixture.create();
    let completed = reserve(&fixture, "upload_complete_rollback", 1_100);
    fixture
        .repository
        .record_uploaded_bytes(&completed.id, 5, &completed.expected_checksum)
        .expect("uploaded bytes");
    clear_reserved_capacity(&fixture);
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &completed.id,
            1_101,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101),
        ),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        states(&fixture, &completed.id),
        (
            "uploaded".to_owned(),
            "pending".to_owned(),
            "reserved".to_owned(),
        )
    );

    let aborted = reserve(&fixture, "upload_abort_rollback", 1_200);
    clear_reserved_capacity(&fixture);
    assert_eq!(
        fixture
            .repository
            .abort_inbox_upload(&fixture.inbox.capability_hash, &aborted.id, 1_201,),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        states(&fixture, &aborted.id),
        (
            "requested".to_owned(),
            "pending".to_owned(),
            "reserved".to_owned(),
        )
    );
}

#[test]
fn suppressed_link_transition_rolls_back_every_prior_completion_mutation() {
    let fixture = Fixture::new();
    fixture.create();
    let reservation = reserve(&fixture, "upload_link_rollback", 1_100);
    fixture
        .repository
        .record_uploaded_bytes(&reservation.id, 5, &reservation.expected_checksum)
        .expect("uploaded bytes");
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER suppress_inbox_complete BEFORE UPDATE OF status ON inbox_uploads
             WHEN NEW.status = 'complete' BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("suppressing trigger");
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &reservation.id,
            1_101,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101),
        ),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        states(&fixture, &reservation.id),
        (
            "uploaded".to_owned(),
            "pending".to_owned(),
            "reserved".to_owned(),
        )
    );
    let counters = fixture
        .repository
        .list_inboxes(&fixture.inbox.project_id)
        .expect("inboxes")
        .pop()
        .expect("inbox");
    assert_eq!(
        (
            counters.current_files,
            counters.current_bytes,
            counters.reserved_files,
            counters.reserved_bytes,
        ),
        (0, 0, 1, 5)
    );
    let audit = fixture
        .repository
        .list_audit(&fixture.inbox.workspace_id, None, 10)
        .expect("audit");
    assert!(
        !audit
            .items
            .iter()
            .any(|event| event.action == "inbox.uploaded")
    );
}

#[test]
fn revocation_immediately_blocks_every_existing_upload_transition() {
    let fixture = Fixture::new();
    fixture.create();
    let reservation = reserve(&fixture, "upload_revoked", 1_100);
    fixture
        .repository
        .record_uploaded_bytes(&reservation.id, 5, &reservation.expected_checksum)
        .expect("uploaded bytes");
    fixture
        .repository
        .revoke_inbox(
            &fixture.inbox.id,
            &fixture.inbox.workspace_id,
            1_101,
            &event("inbox.revoked", &fixture.inbox, 1_101),
        )
        .expect("revocation");
    assert_eq!(
        fixture.repository.inbox_upload_by_id(
            &fixture.inbox.capability_hash,
            &reservation.id,
            1_102,
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &reservation.id,
            1_102,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_102),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.abort_inbox_upload(
            &fixture.inbox.capability_hash,
            &reservation.id,
            1_102,
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn multipart_abort_deletes_parts() {
    let fixture = Fixture::new();
    fixture.create();
    let mut multipart = upload("upload_multipart_abort", 1_100);
    multipart.strategy = ReservationStrategy::Multipart;
    multipart.part_size = Some(3);
    multipart.part_count = Some(2);
    fixture
        .repository
        .reserve_inbox_upload(&principal(&fixture.inbox, 1_100), &multipart)
        .expect("multipart reservation");
    fixture
        .repository
        .attach_multipart(&multipart.id, "provider-upload")
        .expect("provider upload");
    let grants = [
        NewUploadPartGrant {
            upload_id: multipart.id.clone(),
            part_number: 1,
            expected_size: 3,
            capability_hash: hash('7'),
            expires_at_ms: 3_000,
        },
        NewUploadPartGrant {
            upload_id: multipart.id.clone(),
            part_number: 2,
            expected_size: 2,
            capability_hash: hash('8'),
            expires_at_ms: 3_000,
        },
    ];
    assert_eq!(
        fixture
            .repository
            .issue_upload_parts(&grants)
            .expect("part grants")
            .len(),
        2
    );
    fixture
        .repository
        .abort_inbox_upload(&fixture.inbox.capability_hash, &multipart.id, 1_101)
        .expect("multipart abort");
    assert!(
        fixture
            .repository
            .list_upload_parts(&multipart.id)
            .expect("remaining parts")
            .is_empty()
    );
}

#[test]
fn successful_audit_matches_hosted_contract() {
    let fixture = Fixture::new();
    fixture.create();
    let completed = reserve(&fixture, "upload_hosted_audit", 1_200);
    fixture
        .repository
        .record_uploaded_bytes(&completed.id, 5, &completed.expected_checksum)
        .expect("uploaded bytes");
    let version = fixture
        .repository
        .complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &completed.id,
            1_201,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_201),
        )
        .expect("completed inbox upload");
    assert_eq!(version.state, UploadState::Complete);
    let audit = fixture
        .repository
        .list_audit(&fixture.inbox.workspace_id, None, 10)
        .expect("audit");
    let uploaded = audit
        .items
        .iter()
        .find(|item| item.action == "inbox.uploaded")
        .expect("upload audit");
    assert_eq!(uploaded.actor, fixture.inbox.id);
    assert_eq!(
        uploaded.metadata,
        vec![
            ("byteSize".to_owned(), AuditValue::Number(5)),
            ("source".to_owned(), AuditValue::String("inbox".to_owned()),),
        ]
    );
}
