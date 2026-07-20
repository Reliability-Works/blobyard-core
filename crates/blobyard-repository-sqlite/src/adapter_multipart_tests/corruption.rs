use super::*;

#[test]
fn multipart_listing_maps_corrupt_durable_part_rows() {
    let (_temporary, repository, upload) = attached_multipart();
    repository
        .issue_upload_parts(&[part(1, 3, 'c')])
        .expect("part grant");
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = ON")
        .expect("disable check constraints");
    connection
        .execute(
            "UPDATE upload_parts SET expected_size = -1 WHERE upload_id = ?1",
            [&upload.id],
        )
        .expect("corrupt part row");
    drop(connection);
    assert_eq!(
        repository.list_upload_parts(&upload.id),
        Err(RepositoryError::Unavailable)
    );

    let connection = repository.test_connection().expect("connection");
    connection
        .execute(
            "UPDATE upload_parts SET expected_size = 3, provider_tag = CAST(X'80' AS TEXT) WHERE upload_id = ?1",
            [&upload.id],
        )
        .expect("corrupt provider tag");
    drop(connection);
    assert_eq!(
        repository.list_upload_parts(&upload.id),
        Err(RepositoryError::Unavailable)
    );
}
