#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{auth::hash, error::ApiError, transfers::test_seams::TransferFixture};
use axum::{http::HeaderMap, response::IntoResponse, response::Response};
use blobyard_api_client::{DeleteObjectRequest, ListObjectsQuery};
use blobyard_contract::{NewUploadReservation, RepositoryError, StoredObjectRecord};
use blobyard_core::{BlobyardUri, SecretString};

/// Deterministic failures before a download grant may be persisted.
#[derive(Clone, Copy)]
pub enum RequestFailure {
    /// The system clock cannot provide a timestamp.
    Clock,
    /// The grant expiry overflows the timestamp representation.
    ExpiryOverflow,
    /// The configured origin cannot form a secret-bearing URL.
    TransferUrl,
    /// Complete object metadata is missing its byte length.
    MissingSize,
    /// Complete object metadata is missing its checksum.
    MissingChecksum,
    /// The grant expiry cannot be formatted as RFC 3339.
    ExpiryFormat,
}

/// Deterministic failures before a capability lookup or storage read.
#[derive(Clone, Copy)]
pub enum DownloadFailure {
    /// The external capability is malformed.
    MalformedCapability,
    /// The system clock cannot provide a timestamp.
    Clock,
}

impl TransferFixture {
    /// Returns durable object and audit counts for rejected-route assertions.
    #[must_use]
    pub fn object_audit_counts(&self) -> (usize, usize) {
        (objects(self).len(), audits(self).len())
    }

    /// Exercises corrupt persisted object metadata during list response conversion.
    #[must_use]
    pub fn list_corrupt_record_failure(&self) -> Response {
        let mut object = seed_object(self);
        object.version.version = 0;
        let query = list_query();
        super::object_page(&query, Ok(vec![object]))
            .err()
            .expect("corrupt list record")
            .into_response()
    }

    /// Exercises a download-request preparation failure and proves there are no durable effects.
    #[must_use]
    pub fn request_download_failure(&self, failure: RequestFailure) -> Response {
        let mut state = self.state.clone();
        let mut object = seed_object(self);
        let before = objects(self);
        let capability = SecretString::new("download-test-capability").expect("capability");
        let now = match failure {
            RequestFailure::Clock => Err(ApiError::internal()),
            RequestFailure::ExpiryOverflow => Ok(u64::MAX),
            RequestFailure::TransferUrl => {
                "invalid\norigin".clone_into(&mut state.public_origin);
                Ok(0)
            }
            RequestFailure::MissingSize => {
                object.version.size = None;
                Ok(0)
            }
            RequestFailure::MissingChecksum => {
                object.version.checksum = None;
                Ok(0)
            }
            RequestFailure::ExpiryFormat => Ok(u64::MAX - super::grants::GRANT_LIFETIME_MS),
        };
        let result = super::request_download_at(&state, &self.principal, &object, &capability, now);
        assert_eq!(objects(self), before);
        assert_eq!(
            self.state
                .repository
                .download_by_capability(&hash(capability.expose_secret()), 0),
            Err(RepositoryError::NotFound)
        );
        assert!(audits(self).is_empty());
        result
            .err()
            .expect("download request failure")
            .into_response()
    }

    /// Issues one valid grant after preparation and verifies its grant and audit effects.
    #[must_use]
    pub fn request_download_success(&self) -> Response {
        let object = seed_object(self);
        let capability = SecretString::new("download-test-capability").expect("capability");
        let result =
            super::request_download_at(&self.state, &self.principal, &object, &capability, Ok(0));
        let granted = self
            .state
            .repository
            .download_by_capability(&hash(capability.expose_secret()), 0)
            .expect("download grant");
        assert_eq!(granted, object);
        let audits = audits(self);
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].action, "transfer.download_requested");
        result.expect("download request").into_response()
    }

    /// Exercises a clock failure before an object-deletion operation can begin.
    #[must_use]
    pub fn delete_clock_failure(&self) -> Response {
        let before = objects(self);
        let request = DeleteObjectRequest {
            uri: "blobyard://fixture/project/object.bin"
                .parse::<BlobyardUri>()
                .expect("object URI"),
        };
        let result = super::delete_object_at(
            &self.state,
            crate::auth::Principal(self.principal.clone()),
            request,
            Err(ApiError::internal()),
        );
        assert_eq!(objects(self), before);
        assert!(audits(self).is_empty());
        result
            .err()
            .expect("deletion clock failure")
            .into_response()
    }

    /// Exercises a pre-lookup download failure.
    pub async fn download_failure(&self, failure: DownloadFailure) -> Response {
        let (capability, now) = match failure {
            DownloadFailure::MalformedCapability => (
                ApiError::not_found_result(SecretString::new("invalid\ncapability")),
                Ok(0),
            ),
            DownloadFailure::Clock => (
                Ok(SecretString::new("valid-capability").expect("capability")),
                Err(ApiError::internal()),
            ),
        };
        super::download_at(&self.state, capability, &HeaderMap::new(), now)
            .await
            .expect_err("download failure")
            .into_response()
    }
}

/// Exercises list-result provider error mapping without changing durable state.
#[must_use]
pub fn list_repository_failure() -> Response {
    let query = list_query();
    super::object_page(&query, Err(RepositoryError::Unavailable))
        .err()
        .expect("list failure")
        .into_response()
}

fn list_query() -> ListObjectsQuery {
    ListObjectsQuery {
        workspace: "fixture".parse().expect("workspace slug"),
        project: "project".parse().expect("project slug"),
        prefix: None,
        versions: false,
        cursor: None,
    }
}

fn seed_object(fixture: &TransferFixture) -> StoredObjectRecord {
    let checksum = "00".repeat(32);
    let input = NewUploadReservation {
        id: "download_object".to_owned(),
        project_id: fixture.project.id.clone(),
        object_path: "object.bin".to_owned(),
        filename: "object.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size: 1,
        expected_checksum: checksum.clone(),
        storage_key: "versions/project_fixture/download_object".to_owned(),
        capability_hash: hash("upload-capability"),
        expires_at_ms: 1,
        created_at_ms: 0,
        source: blobyard_contract::ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    };
    fixture
        .state
        .repository
        .reserve_upload(&input)
        .expect("upload reservation");
    fixture
        .state
        .repository
        .record_uploaded_bytes(&input.id, 1, &checksum)
        .expect("uploaded bytes");
    fixture
        .state
        .repository
        .complete_upload(&input.id)
        .expect("completed object");
    objects(fixture).into_iter().next().expect("stored object")
}

fn objects(fixture: &TransferFixture) -> Vec<StoredObjectRecord> {
    fixture
        .state
        .repository
        .list_stored_objects(&fixture.project.id, None, true)
        .expect("stored objects")
}

fn audits(fixture: &TransferFixture) -> Vec<blobyard_contract::AuditEventRecord> {
    fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 50)
        .expect("audit query")
        .items
}
