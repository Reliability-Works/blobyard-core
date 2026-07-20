use super::*;

#[test]
fn provider_upload_locator_uses_its_own_bounded_validation_contract() {
    let (_temporary, repository, upload) = reserved_multipart();
    let maximum = "p".repeat(4_096);
    let attached = repository
        .attach_multipart(&upload.id, &maximum)
        .expect("maximum provider locator");
    assert_eq!(
        attached.provider_upload_id.as_deref(),
        Some(maximum.as_str())
    );

    for invalid in [
        String::new(),
        "p".repeat(4_097),
        "invalid\nprovider".to_owned(),
    ] {
        let (_temporary, repository, upload) = reserved_multipart();
        assert_eq!(
            repository.attach_multipart(&upload.id, &invalid),
            Err(RepositoryError::InvalidInput)
        );
    }
}
