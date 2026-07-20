use super::*;

#[test]
fn uploaded_parts_are_exact_replaceable_sorted_and_removed_on_abort() {
    let (_temporary, repository, upload) = attached_multipart();
    repository
        .issue_upload_parts(&[part(2, 2, 'd'), part(1, 3, 'c')])
        .expect("part grants");
    assert_eq!(
        repository.record_uploaded_part(&upload.id, 1, 2, &checksum('a'), Some("wrong-size")),
        Err(RepositoryError::InvalidInput)
    );
    repository
        .record_uploaded_part(&upload.id, 2, 2, &checksum('d'), Some("tag-two"))
        .expect("part two");
    repository
        .record_uploaded_part(&upload.id, 1, 3, &checksum('a'), Some("tag-one-old"))
        .expect("part one");
    repository
        .record_uploaded_part(&upload.id, 1, 3, &checksum('b'), Some("tag-one"))
        .expect("replacement part one");
    let listed = repository.list_upload_parts(&upload.id).expect("parts");
    assert_eq!(
        listed
            .iter()
            .map(|record| record.part_number)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(listed[0].provider_tag.as_deref(), Some("tag-one"));
    assert_eq!(listed[1].provider_tag.as_deref(), Some("tag-two"));
    assert_eq!(
        listed[0].received_checksum.as_deref(),
        Some(checksum('b').as_str())
    );
    repository.abort_upload(&upload.id).expect("abort");
    assert!(
        repository
            .list_upload_parts(&upload.id)
            .expect("parts")
            .is_empty()
    );
}
