use super::*;
use blobyard_contract::{InboxRepository, ObjectSource, ReservationState, TransferRepository};

type ReservationMutation = fn(&mut NewInboxUpload, &mut NewUploadReservation);

#[test]
fn reservation_rejects_invalid_principal_and_scope() {
    let fixture = Fixture::new();
    fixture.create();
    let valid = upload("upload_valid", 1_100);
    let mutations: [ReservationMutation; 8] = [
        |principal, _upload| principal.capability_hash = "invalid".to_owned(),
        |principal, _upload| principal.fingerprint_hash = "invalid".to_owned(),
        |_principal, upload| upload.project_id = "missing".to_owned(),
        |_principal, upload| upload.source = ObjectSource::Cli,
        |_principal, upload| upload.expires_at_ms = 5_001,
        |_principal, upload| upload.expires_at_ms = upload.created_at_ms,
        |principal, _upload| principal.now_ms = 1_101,
        |principal, _upload| principal.now_ms = u64::MAX,
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut principal = principal(&fixture.inbox, 1_100);
        let mut invalid = valid.clone();
        invalid.id = format!("upload_invalid_{index}");
        invalid.storage_key = format!("objects/upload_invalid_{index}");
        invalid.object_path = format!("inbox/upload_invalid_{index}.bin");
        invalid.capability_hash = format!("{:064x}", index + 16);
        mutate(&mut principal, &mut invalid);
        assert_eq!(
            fixture
                .repository
                .reserve_inbox_upload(&principal, &invalid),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn reservation_replay_rolls_back_capacity_and_queries_conceal_tokens() {
    let fixture = Fixture::new();
    fixture.create();
    let reservation = upload("upload_replay", 1_100);
    let principal = principal(&fixture.inbox, 1_100);
    fixture
        .repository
        .reserve_inbox_upload(&principal, &reservation)
        .expect("reservation");
    assert_eq!(
        fixture
            .repository
            .reserve_inbox_upload(&principal, &reservation),
        Err(RepositoryError::Conflict)
    );
    let counters = fixture
        .repository
        .list_inboxes(&fixture.inbox.project_id)
        .expect("inboxes")[0]
        .clone();
    assert_eq!((counters.reserved_files, counters.reserved_bytes), (1, 5));
    assert_eq!(
        fixture
            .repository
            .inbox_upload_by_id(&hash('e'), &reservation.id, 1_101),
        Err(RepositoryError::NotFound)
    );
    for (capability, upload_id, now) in [
        ("invalid", reservation.id.as_str(), 1_101),
        (fixture.inbox.capability_hash.as_str(), "", 1_101),
        (
            fixture.inbox.capability_hash.as_str(),
            reservation.id.as_str(),
            u64::MAX,
        ),
    ] {
        assert_eq!(
            fixture
                .repository
                .inbox_upload_by_id(capability, upload_id, now),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn completion_fails_closed_at_each_state_boundary() {
    let fixture = Fixture::new();
    fixture.create();
    let complete = upload("upload_complete", 1_100);
    fixture
        .repository
        .reserve_inbox_upload(&principal(&fixture.inbox, 1_100), &complete)
        .expect("complete reservation");
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &complete.id,
            1_101,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101),
        ),
        Err(RepositoryError::Conflict)
    );
    fixture
        .repository
        .record_uploaded_bytes(&complete.id, 5, &complete.expected_checksum)
        .expect("uploaded bytes");
    let mut invalid_event = blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101);
    invalid_event.action = "inbox.wrong".to_owned();
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &complete.id,
            1_101,
            &invalid_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
    fixture
        .repository
        .complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &complete.id,
            1_101,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101),
        )
        .expect("completion");
    assert_eq!(
        fixture
            .repository
            .abort_inbox_upload(&fixture.inbox.capability_hash, &complete.id, 1_102,),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn abort_fails_closed_at_each_state_boundary() {
    let fixture = Fixture::new();
    fixture.create();
    let aborted = upload("upload_abort", 1_200);
    let mut abort_principal = principal(&fixture.inbox, 1_200);
    abort_principal.fingerprint_hash = hash('f');
    let mut aborted = aborted;
    aborted.capability_hash = format!("{}2", "0".repeat(63));
    fixture
        .repository
        .reserve_inbox_upload(&abort_principal, &aborted)
        .expect("abort reservation");
    let prior = fixture
        .repository
        .abort_inbox_upload(&fixture.inbox.capability_hash, &aborted.id, 1_201)
        .expect("abort");
    assert_eq!(prior.state, ReservationState::Requested);
    assert_eq!(
        fixture
            .repository
            .abort_inbox_upload(&fixture.inbox.capability_hash, &aborted.id, 1_202,),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn completion_and_abort_reject_invalid_access_and_non_inbox_sources() {
    let fixture = Fixture::new();
    fixture.create();
    for (capability, upload_id, now) in [
        ("invalid", "upload", 1_100),
        (fixture.inbox.capability_hash.as_str(), "", 1_100),
        (fixture.inbox.capability_hash.as_str(), "upload", u64::MAX),
    ] {
        assert_eq!(
            fixture.repository.complete_inbox_upload(
                capability,
                upload_id,
                now,
                &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, now),
            ),
            Err(RepositoryError::InvalidInput)
        );
        assert_eq!(
            fixture
                .repository
                .abort_inbox_upload(capability, upload_id, now),
            Err(RepositoryError::InvalidInput)
        );
    }

    let mut foreign = upload("upload_foreign_source", 1_100);
    foreign.source = ObjectSource::Cli;
    fixture
        .repository
        .reserve_upload(&foreign)
        .expect("ordinary reservation");
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "INSERT INTO inbox_uploads (upload_id, inbox_id, fingerprint_hash, reserved_size, status, created_at_ms) VALUES (?1, ?2, ?3, ?4, 'reserved', ?5)",
            rusqlite::params![foreign.id, fixture.inbox.id, hash('9'), 5, 1_100],
        )
        .expect("inbox association");
    assert_eq!(
        fixture.repository.complete_inbox_upload(
            &fixture.inbox.capability_hash,
            &foreign.id,
            1_101,
            &blobyard_testkit::inbox_upload_event(&fixture.inbox.id, 1_101),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .repository
            .abort_inbox_upload(&fixture.inbox.capability_hash, &foreign.id, 1_101,),
        Err(RepositoryError::NotFound)
    );
}
