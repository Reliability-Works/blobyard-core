use super::{Corrupting, Corruption};
use blobyard_contract::{
    NewDownloadGrant, NewUploadPartGrant, NewUploadReservation, ObjectVersionRecord,
    RepositoryError, ReservationState, StoredObjectRecord, TransferRepository, UploadPartRecord,
    UploadReservationRecord, UploadState,
};

impl<T: TransferRepository> TransferRepository for Corrupting<'_, T> {
    fn reserve_upload(
        &self,
        value: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner.reserve_upload(value).map(|mut record| {
            match self.corruption {
                Corruption::FirstReservationVersion if value.id == "upload_one" => {
                    record.version.version = 9;
                }
                Corruption::FirstReservationState if value.id == "upload_one" => {
                    record.state = ReservationState::Aborted;
                }
                Corruption::SecondReservationVersion if value.id == "upload_two" => {
                    record.version.version = 9;
                }
                Corruption::MultipartReservation if value.id == "upload_multipart" => {
                    record.part_count = None;
                }
                _ => {}
            }
            record
        })
    }

    fn upload_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner
            .upload_by_capability(hash, now_ms)
            .map(|mut record| {
                match self.corruption {
                    Corruption::FirstCapabilityRecord if now_ms == 999 => {
                        record.filename.push_str(" changed");
                    }
                    Corruption::RenewedExpiry if now_ms == 1_999 => record.expires_at_ms = 9,
                    _ => {}
                }
                record
            })
    }

    fn upload_by_id(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner.upload_by_id(id).map(|mut record| {
            match self.corruption {
                Corruption::RequestedAbortStored if id == "upload_abort_requested" => {
                    record.state = ReservationState::Requested;
                }
                Corruption::UploadedAbortStored if id == "upload_abort_uploaded" => {
                    record.state = ReservationState::Uploaded;
                }
                _ => {}
            }
            record
        })
    }

    fn renew_upload(&self, id: &str, expires_at_ms: u64) -> Result<(), RepositoryError> {
        self.inner.renew_upload(id, expires_at_ms)
    }

    fn attach_multipart(
        &self,
        id: &str,
        provider_upload_id: &str,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner
            .attach_multipart(id, provider_upload_id)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::MultipartAttachment)
                    && id == "upload_multipart"
                {
                    record.provider_upload_id = Some("wrong-provider".to_owned());
                }
                record
            })
    }

    fn issue_upload_parts(
        &self,
        parts: &[NewUploadPartGrant],
    ) -> Result<Vec<UploadPartRecord>, RepositoryError> {
        self.inner.issue_upload_parts(parts).map(|mut records| {
            if matches!(self.corruption, Corruption::MultipartIssued)
                && parts
                    .first()
                    .is_some_and(|part| part.upload_id == "upload_multipart")
            {
                records.clear();
            }
            records
        })
    }

    fn upload_part_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<(UploadReservationRecord, UploadPartRecord), RepositoryError> {
        self.inner
            .upload_part_by_capability(hash, now_ms)
            .map(|(mut upload, part)| {
                if matches!(self.corruption, Corruption::MultipartResolution) {
                    "wrong-upload".clone_into(&mut upload.id);
                }
                (upload, part)
            })
    }

    fn record_uploaded_part(
        &self,
        id: &str,
        part_number: u32,
        size: u64,
        checksum: &str,
        provider_tag: Option<&str>,
    ) -> Result<(), RepositoryError> {
        self.inner
            .record_uploaded_part(id, part_number, size, checksum, provider_tag)
    }

    fn list_upload_parts(&self, id: &str) -> Result<Vec<UploadPartRecord>, RepositoryError> {
        self.inner.list_upload_parts(id).map(|mut records| {
            if matches!(self.corruption, Corruption::MultipartListing) && id == "upload_multipart" {
                records.clear();
            }
            records
        })
    }

    fn record_uploaded_bytes(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError> {
        self.inner.record_uploaded_bytes(id, size, checksum)
    }

    fn complete_upload(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        self.inner.complete_upload(id).map(|mut record| {
            match self.corruption {
                Corruption::CompletedState if id == "upload_one" => {
                    record.state = UploadState::Pending;
                }
                Corruption::CompletedVersion if id == "upload_one" => record.version = 9,
                Corruption::MultipartCompletion if id == "upload_multipart" => {
                    record.state = UploadState::Pending;
                }
                _ => {}
            }
            record
        })
    }

    fn abort_upload(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner.abort_upload(id).map(|mut record| {
            match self.corruption {
                Corruption::RequestedAbortPrior if id == "upload_abort_requested" => {
                    record.state = ReservationState::Complete;
                }
                Corruption::UploadedAbortPrior if id == "upload_abort_uploaded" => {
                    record.state = ReservationState::Requested;
                }
                Corruption::MultipartAbort if id == "upload_multipart_abort" => {
                    record.state = ReservationState::Complete;
                }
                _ => {}
            }
            record
        })
    }

    fn list_stored_objects(
        &self,
        project_id: &str,
        prefix: Option<&str>,
        include_versions: bool,
    ) -> Result<Vec<StoredObjectRecord>, RepositoryError> {
        self.inner
            .list_stored_objects(project_id, prefix, include_versions)
            .map(|mut values| {
                match (self.corruption, prefix) {
                    (
                        Corruption::DownloadList
                        | Corruption::PreviewObjectList
                        | Corruption::YardFixtureObjectList,
                        Some("artifacts/build.zip"),
                    )
                    | (Corruption::LatestLength, Some("artifacts/"))
                    | (Corruption::AllLength, None) => values.clear(),
                    (Corruption::LatestVersion, Some("artifacts/")) => {
                        values[0].version.version = 9;
                    }
                    (Corruption::AllFirstVersion, None) => values[0].version.version = 9,
                    (Corruption::AllSecondVersion, None) => values[1].version.version = 9,
                    _ => {}
                }
                values
            })
    }

    fn issue_download(&self, value: &NewDownloadGrant) -> Result<(), RepositoryError> {
        self.inner.issue_download(value)
    }

    fn download_by_capability(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<StoredObjectRecord, RepositoryError> {
        self.inner
            .download_by_capability(hash, now_ms)
            .map(|mut value| {
                if matches!(self.corruption, Corruption::DownloadVersion) {
                    value.version.version = 9;
                }
                value
            })
    }
}
