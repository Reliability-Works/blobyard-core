#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::{NewUploadPartGrant, ReservationStrategy};

#[path = "adapter_multipart_tests/corruption.rs"]
mod corruption;
#[path = "adapter_multipart_tests/parts.rs"]
mod parts;
#[path = "adapter_multipart_tests/validation.rs"]
mod validation;

fn multipart_upload() -> NewUploadReservation {
    let mut value = upload();
    value.expected_size = 5;
    value.strategy = ReservationStrategy::Multipart;
    value.part_size = Some(3);
    value.part_count = Some(2);
    value
}

fn reserved_multipart() -> (tempfile::TempDir, SqliteRepository, NewUploadReservation) {
    let (temporary, repository) = super::stable_behavior::repository();
    let upload = multipart_upload();
    repository.reserve_upload(&upload).expect("reservation");
    (temporary, repository, upload)
}

fn attached_multipart() -> (tempfile::TempDir, SqliteRepository, NewUploadReservation) {
    let (temporary, repository, upload) = reserved_multipart();
    repository
        .attach_multipart(&upload.id, "provider")
        .expect("provider");
    (temporary, repository, upload)
}

fn part(number: u32, size: u64, character: char) -> NewUploadPartGrant {
    NewUploadPartGrant {
        upload_id: "upload_fixture".to_owned(),
        part_number: number,
        expected_size: size,
        capability_hash: checksum(character),
        expires_at_ms: 10,
    }
}

#[test]
fn multipart_strategy_and_provider_attachment_are_durable_and_idempotent() {
    let (_temporary, repository, upload) = reserved_multipart();
    let reserved = repository.upload_by_id(&upload.id).expect("reservation");
    assert_eq!(reserved.strategy, ReservationStrategy::Multipart);
    assert_eq!(reserved.part_size, Some(3));
    assert_eq!(reserved.part_count, Some(2));
    assert_eq!(reserved.provider_upload_id, None);

    let attached = repository
        .attach_multipart(&reserved.id, "provider-one")
        .expect("provider attachment");
    assert_eq!(attached.provider_upload_id.as_deref(), Some("provider-one"));
    assert_eq!(
        repository.attach_multipart(&reserved.id, "provider-one"),
        Ok(attached)
    );
    assert_eq!(
        repository.attach_multipart(&reserved.id, "provider-two"),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn part_grants_are_bounded_retryable_and_resolved_only_while_active() {
    let (_temporary, repository, _upload) = attached_multipart();
    let issued = repository
        .issue_upload_parts(&[part(2, 2, 'd'), part(1, 3, 'c')])
        .expect("part grants");
    assert_eq!(
        issued
            .iter()
            .map(|record| record.part_number)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        repository
            .upload_part_by_capability(&checksum('c'), 9)
            .expect("active capability")
            .1,
        issued[0]
    );
    assert_eq!(
        repository.upload_part_by_capability(&checksum('c'), 10),
        Err(RepositoryError::NotFound)
    );
    let replacement = part(1, 3, 'e');
    assert_eq!(
        repository
            .issue_upload_parts(std::slice::from_ref(&replacement))
            .expect("replacement")[0]
            .part_number,
        1
    );
    assert_eq!(
        repository.upload_part_by_capability(&checksum('c'), 1),
        Err(RepositoryError::NotFound)
    );
    assert!(
        repository
            .upload_part_by_capability(&replacement.capability_hash, 1)
            .is_ok()
    );
}

#[test]
fn multipart_parts_require_exact_batch_shape_and_integrity() {
    let (_temporary, repository, _upload) = attached_multipart();
    for invalid_parts in [
        Vec::new(),
        vec![part(1, 3, 'c'), part(1, 3, 'd')],
        vec![part(0, 3, 'c')],
        vec![part(3, 1, 'c')],
        vec![part(2, 3, 'c')],
    ] {
        assert_eq!(
            repository.issue_upload_parts(&invalid_parts),
            Err(RepositoryError::InvalidInput)
        );
    }
    let mut foreign = part(2, 2, 'd');
    foreign.upload_id = "foreign".to_owned();
    assert_eq!(
        repository.issue_upload_parts(&[part(1, 3, 'c'), foreign]),
        Err(RepositoryError::InvalidInput)
    );
    let oversized = (1..=101)
        .map(|number| part(number, 3, char::from_digit(number % 10, 10).unwrap_or('a')))
        .collect::<Vec<_>>();
    assert_eq!(
        repository.issue_upload_parts(&oversized),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid_capability = part(1, 3, 'c');
    invalid_capability.capability_hash = "invalid".to_owned();
    let mut oversized_size = part(1, 3, 'c');
    oversized_size.expected_size = u64::MAX;
    let mut oversized_expiry = part(1, 3, 'c');
    oversized_expiry.expires_at_ms = u64::MAX;
    for invalid in [invalid_capability, oversized_size, oversized_expiry] {
        assert_eq!(
            repository.issue_upload_parts(&[invalid]),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn multipart_entry_points_reject_invalid_identifiers_and_inactive_reservations() {
    let (_temporary, repository, upload) = reserved_multipart();
    assert_eq!(
        repository.attach_multipart("invalid\nid", "provider"),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.attach_multipart(&upload.id, "invalid\nprovider"),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.issue_upload_parts(&[part(1, 3, 'c')]),
        Ok(vec![
            repository
                .list_upload_parts(&upload.id)
                .expect("issued parts")[0]
                .clone()
        ])
    );
    repository.abort_upload(&upload.id).expect("abort upload");
    assert_eq!(
        repository.attach_multipart(&upload.id, "provider"),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        repository.issue_upload_parts(&[part(1, 3, 'd')]),
        Err(RepositoryError::Conflict)
    );

    let (_single_root, single_repository) = super::stable_behavior::repository();
    let single = super::upload();
    single_repository
        .reserve_upload(&single)
        .expect("single reservation");
    assert_eq!(
        single_repository.attach_multipart(&single.id, "provider"),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn multipart_capability_recording_and_listing_validate_every_public_bound() {
    let (_temporary, repository, upload) = attached_multipart();
    repository
        .issue_upload_parts(&[part(1, 3, 'c')])
        .expect("part grant");
    assert_eq!(
        repository.upload_part_by_capability("invalid", 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.upload_part_by_capability(&checksum('c'), u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.record_uploaded_part("invalid\nid", 1, 3, &checksum('a'), None),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.record_uploaded_part(&upload.id, 1, 3, "invalid", None),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.record_uploaded_part(&upload.id, 0, 3, &checksum('a'), None),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.record_uploaded_part(&upload.id, 1, u64::MAX, &checksum('a'), None),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.record_uploaded_part(&upload.id, 1, 3, &checksum('a'), Some("invalid\ntag"),),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_upload_parts("invalid\nid"),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_upload_parts("missing"),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn upload_reservation_rejects_each_invalid_multipart_strategy_shape() {
    let (_temporary, repository) = super::stable_behavior::repository();
    for (size, count) in [
        (None, None),
        (Some(0), Some(1)),
        (Some(3), Some(0)),
        (Some(3), Some(10_001)),
        (Some(3), Some(3)),
    ] {
        let mut upload = multipart_upload();
        upload.id = format!("upload_{size:?}_{count:?}");
        upload.storage_key = format!("objects/{}", upload.id);
        upload.part_size = size;
        upload.part_count = count;
        assert_eq!(
            repository.reserve_upload(&upload),
            Err(RepositoryError::InvalidInput)
        );
    }
    let mut oversized_part = multipart_upload();
    oversized_part.expected_size = 5;
    oversized_part.part_size = Some(u64::MAX);
    oversized_part.part_count = Some(1);
    assert_eq!(
        repository.reserve_upload(&oversized_part),
        Err(RepositoryError::InvalidInput)
    );
}
