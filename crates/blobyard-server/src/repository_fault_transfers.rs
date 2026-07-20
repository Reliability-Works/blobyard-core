use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    NewDownloadGrant, NewUploadPartGrant, NewUploadReservation, ObjectVersionRecord,
    RepositoryError, StoredObjectRecord, TransferRepository, UploadPartRecord,
    UploadReservationRecord,
};

impl TransferRepository for FaultingRepository {
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
        let mut record = self.inner.upload_by_id(id)?;
        if matches!(self.corruption, Some(Corruption::AbortedStorageKey)) {
            record.version.storage_key = "../invalid".to_owned();
        }
        Ok(record)
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
        let mut record = self.inner.complete_upload(id)?;
        match self.corruption {
            Some(Corruption::CompletedVersion) => record.version = 0,
            Some(Corruption::CompletedPath) => record.object_path = "/absolute".to_owned(),
            Some(Corruption::CompletedSize) => record.size = None,
            Some(Corruption::CompletedChecksum) => record.checksum = None,
            Some(
                Corruption::AbortedStorageKey
                | Corruption::ShareObjectSize
                | Corruption::ShareExpiry
                | Corruption::InboxExpiry
                | Corruption::PreviewCreatedAt
                | Corruption::PreviewExpiresAt,
            )
            | None => {}
        }
        Ok(record)
    }

    fn abort_upload(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        let mut record = self.inner.abort_upload(id)?;
        if matches!(self.corruption, Some(Corruption::AbortedStorageKey)) {
            record.version.storage_key = "../invalid".to_owned();
        }
        Ok(record)
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
