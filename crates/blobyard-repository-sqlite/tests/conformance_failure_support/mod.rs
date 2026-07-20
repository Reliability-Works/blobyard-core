#![allow(clippy::expect_used, reason = "test synchronization must fail loudly")]

use blobyard_contract::{
    CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord, MetadataRepository,
    NewAuditEvent, NewDownloadGrant, NewUploadPartGrant, NewUploadReservation, ObjectVersionRecord,
    RepositoryError, StoredObjectRecord, TransferRepository, UploadPartRecord,
    UploadReservationRecord,
};
use blobyard_repository_sqlite::SqliteRepository;

/// Result-corruption adapters for conformance assertions.
pub mod corrupting;
pub(crate) use corrupting::{Corrupting, Corruption};
mod faulting_inboxes;
mod faulting_lifecycle;
mod faulting_previews;
mod faulting_sharing;
mod faulting_yards;

pub(crate) struct Faulting<'a, T> {
    inner: &'a T,
    failures: blobyard_testkit::FailureCounter,
}

pub(crate) fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    (temporary, repository)
}

pub(crate) fn yard_fixture() -> blobyard_testkit::YardConformanceFixture {
    blobyard_testkit::YardConformanceFixture::new("docs", "inactive", "history")
        .expect("Yard conformance fixture")
}

pub(crate) fn every_operation_fails_closed(
    mut run: impl FnMut(usize) -> Result<(), RepositoryError>,
) {
    let successful_index = (0..128).find(|&failure_index| run(failure_index).is_ok());
    assert!(successful_index.is_some(), "conformance must terminate");
    assert_ne!(
        successful_index,
        Some(0),
        "conformance must exercise operations"
    );
}

impl<'a, T> Faulting<'a, T> {
    pub(crate) const fn new(inner: &'a T, failure_index: usize) -> Self {
        Self {
            inner,
            failures: blobyard_testkit::FailureCounter::new(failure_index),
        }
    }

    fn check(&self) -> Result<(), RepositoryError> {
        self.failures.check()
    }
}

impl<T: MetadataRepository> MetadataRepository for Faulting<'_, T> {
    blobyard_testkit::impl_faulting_metadata_repository!();
}

impl<T: CredentialRepository> CredentialRepository for Faulting<'_, T> {
    fn install_bootstrap(&self, hash: &str) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner.install_bootstrap(hash)
    }

    fn exchange_bootstrap(
        &self,
        hash: &str,
        token: &LocalApiTokenRecord,
        session: &LocalCliSessionRecord,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.exchange_bootstrap(hash, token, session)
    }

    fn list_cli_sessions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCliSessionRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_cli_sessions(workspace_id)
    }

    fn revoke_cli_session(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .revoke_cli_session(id, workspace_id, now_ms, event)
    }

    fn create_api_token(
        &self,
        token: &LocalApiTokenRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_api_token(token, event)
    }

    fn list_api_tokens(&self) -> Result<Vec<LocalApiTokenRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_api_tokens()
    }

    fn authenticate_api_token(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<LocalApiTokenRecord, RepositoryError> {
        self.check()?;
        self.inner.authenticate_api_token(hash, now_ms)
    }

    fn revoke_api_token(
        &self,
        id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.revoke_api_token(id, now_ms, event)
    }
}

impl<T: TransferRepository> TransferRepository for Faulting<'_, T> {
    fn reserve_upload(
        &self,
        value: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.reserve_upload(value)
    }

    fn upload_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.upload_by_capability(hash, now_ms)
    }

    fn upload_by_id(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.upload_by_id(id)
    }

    fn renew_upload(&self, id: &str, expires_at_ms: u64) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.renew_upload(id, expires_at_ms)
    }

    fn attach_multipart(
        &self,
        id: &str,
        provider_upload_id: &str,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.attach_multipart(id, provider_upload_id)
    }

    fn issue_upload_parts(
        &self,
        parts: &[NewUploadPartGrant],
    ) -> Result<Vec<UploadPartRecord>, RepositoryError> {
        self.check()?;
        self.inner.issue_upload_parts(parts)
    }

    fn upload_part_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<(UploadReservationRecord, UploadPartRecord), RepositoryError> {
        self.check()?;
        self.inner.upload_part_by_capability(hash, now_ms)
    }

    fn record_uploaded_part(
        &self,
        id: &str,
        part_number: u32,
        size: u64,
        checksum: &str,
        provider_tag: Option<&str>,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .record_uploaded_part(id, part_number, size, checksum, provider_tag)
    }

    fn list_upload_parts(&self, id: &str) -> Result<Vec<UploadPartRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_upload_parts(id)
    }

    fn record_uploaded_bytes(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.record_uploaded_bytes(id, size, checksum)
    }

    fn complete_upload(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        self.check()?;
        self.inner.complete_upload(id)
    }

    fn abort_upload(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.abort_upload(id)
    }

    fn list_stored_objects(
        &self,
        project_id: &str,
        prefix: Option<&str>,
        include_versions: bool,
    ) -> Result<Vec<StoredObjectRecord>, RepositoryError> {
        self.check()?;
        self.inner
            .list_stored_objects(project_id, prefix, include_versions)
    }

    fn issue_download(&self, value: &NewDownloadGrant) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.issue_download(value)
    }

    fn download_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<StoredObjectRecord, RepositoryError> {
        self.check()?;
        self.inner.download_by_capability(hash, now_ms)
    }
}
