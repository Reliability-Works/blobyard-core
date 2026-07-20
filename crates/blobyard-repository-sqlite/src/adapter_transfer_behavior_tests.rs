#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{audit, stable_behavior};
use blobyard_contract::{
    CredentialRepository, MetadataRepository, NewDownloadGrant, RepositoryError, TransferRepository,
};

#[test]
fn transfer_state_failures_keep_their_exact_contract() {
    let (_temporary, repository) = stable_behavior::repository();
    assert_eq!(
        repository.install_bootstrap("invalid"),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_api_token("missing", 1, &audit()),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.complete_object_version("missing", 1, &stable_behavior::checksum('a')),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.issue_download(&NewDownloadGrant {
            version_id: "missing".to_owned(),
            capability_hash: stable_behavior::checksum('b'),
            expires_at_ms: 2,
        }),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.renew_upload("missing", 2),
        Err(RepositoryError::Conflict)
    );

    let mut missing_upload = super::upload();
    missing_upload.id = "upload_missing".to_owned();
    missing_upload.storage_key = "objects/upload_missing".to_owned();
    missing_upload.project_id = "missing".to_owned();
    assert_eq!(
        repository.reserve_upload(&missing_upload),
        Err(RepositoryError::NotFound)
    );
    let upload = super::upload();
    repository.reserve_upload(&upload).expect("upload");
    assert_eq!(
        repository.complete_upload(&upload.id),
        Err(RepositoryError::Conflict)
    );
}
