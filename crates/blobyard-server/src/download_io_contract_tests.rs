#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use axum::{
    body::Body,
    http::{HeaderMap, Method, StatusCode, header},
};
use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, ObjectVersionRecord,
    StorageError, StorageKey, StorageMetadata, StorageRead, StoredObjectRecord, UploadState,
};
use http_body_util::BodyExt;
use std::io::Read;
use std::sync::Arc;

use crate::storage_multipart_macro::storage_multipart_error;
use crate::storage_put_macro::storage_put_error;

type TestResponse = axum::response::Response<Body>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StorageBehavior {
    Success,
    HeadPanic,
    GetPanic,
    HeadError(StorageError),
    GetError(StorageError),
}

#[derive(Debug)]
struct ContractStorage(StorageBehavior);

impl ObjectStorage for ContractStorage {
    storage_put_error!(StorageError::Unavailable);

    fn get(
        &self,
        _key: &StorageKey,
        _range: Option<ByteRange>,
    ) -> Result<StorageRead, StorageError> {
        assert_ne!(self.0, StorageBehavior::GetPanic, "fixture get panic");
        match self.0 {
            StorageBehavior::Success => Ok(StorageRead {
                reader: Box::new(std::io::Cursor::new(b"data")),
                metadata: metadata(),
                range: ByteRange { start: 0, end: 4 },
            }),
            StorageBehavior::GetPanic => Err(StorageError::Unavailable),
            StorageBehavior::GetError(error) => Err(error),
            StorageBehavior::HeadPanic | StorageBehavior::HeadError(_) => {
                Err(StorageError::NotFound)
            }
        }
    }

    fn head(&self, _key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        assert_ne!(self.0, StorageBehavior::HeadPanic, "fixture head panic");
        match self.0 {
            StorageBehavior::HeadPanic => Err(StorageError::Unavailable),
            StorageBehavior::HeadError(error) => Err(error),
            StorageBehavior::Success | StorageBehavior::GetPanic | StorageBehavior::GetError(_) => {
                Ok(metadata())
            }
        }
    }

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        Err(StorageError::Unavailable)
    }

    storage_multipart_error!(StorageError::Unavailable);
}

fn metadata() -> StorageMetadata {
    StorageMetadata {
        size: 4,
        checksum: ObjectChecksum::new("00".repeat(32)).expect("checksum"),
    }
}

fn stored_object(storage_key: &str) -> StoredObjectRecord {
    StoredObjectRecord {
        version: ObjectVersionRecord {
            id: "version_fixture".to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "builds/app.zip".to_owned(),
            version: 1,
            storage_key: storage_key.to_owned(),
            state: UploadState::Complete,
            size: Some(4),
            checksum: Some("00".repeat(32)),
            created_at_ms: 0,
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        },
        filename: "app.zip".to_owned(),
        content_type: "application/zip".to_owned(),
    }
}

async fn response(behavior: StorageBehavior, storage_key: &str) -> axum::response::Response<Body> {
    blobyard_server::download_io::test_seams::response(
        Arc::new(ContractStorage(behavior)),
        &stored_object(storage_key),
        &axum::http::HeaderMap::new(),
    )
    .await
}

async fn public_response(object: &StoredObjectRecord, method: &Method) -> TestResponse {
    public_response_with(StorageBehavior::Success, object, method).await
}

async fn public_response_with(
    behavior: StorageBehavior,
    object: &StoredObjectRecord,
    method: &Method,
) -> axum::response::Response<Body> {
    blobyard_server::download_io::test_seams::public_site_response(
        Arc::new(ContractStorage(behavior)),
        object,
        &HeaderMap::new(),
        method,
    )
    .await
}

async fn assert_error(
    response: axum::response::Response<Body>,
    expected_status: StatusCode,
    expected_code: &str,
    expected_message: &str,
) {
    assert_eq!(response.status(), expected_status);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("error body")
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).expect("error JSON");
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], expected_code);
    assert_eq!(body["error"]["message"], expected_message);
    assert!(
        body["requestId"]
            .as_str()
            .is_some_and(|request_id| request_id.starts_with("req_"))
    );
}

#[tokio::test]
async fn download_adapter_redacts_panics_corrupt_keys_and_provider_failures() {
    let internal_message = "Blobyard couldn't complete that. Try again or contact support.";
    for behavior in [StorageBehavior::HeadPanic, StorageBehavior::GetPanic] {
        assert_error(
            response(behavior, "valid/key").await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            internal_message,
        )
        .await;
    }
    assert_error(
        response(
            StorageBehavior::HeadError(StorageError::NotFound),
            "../invalid",
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        internal_message,
    )
    .await;
    for error in [
        StorageError::Conflict,
        StorageError::InvalidInput,
        StorageError::IntegrityMismatch,
        StorageError::Unavailable,
    ] {
        for behavior in [
            StorageBehavior::HeadError(error),
            StorageBehavior::GetError(error),
        ] {
            assert_error(
                response(behavior, "valid/key").await,
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                internal_message,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn download_adapter_preserves_provider_not_found() {
    for behavior in [
        StorageBehavior::HeadError(StorageError::NotFound),
        StorageBehavior::GetError(StorageError::NotFound),
    ] {
        assert_error(
            response(behavior, "valid/key").await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "That item couldn't be found. Check the name and try again.",
        )
        .await;
    }
}

#[tokio::test]
async fn download_adapter_preserves_successful_full_responses() {
    let response = response(StorageBehavior::Success, "valid/key").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[axum::http::header::CONTENT_LENGTH], "4");
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .expect("response body")
            .to_bytes(),
        b"data".as_slice()
    );
}

#[tokio::test]
async fn public_site_adapter_sets_isolation_headers_and_removes_head_bodies() {
    let object = stored_object("valid/key");
    let response = public_response(&object, &Method::GET).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::CONTENT_DISPOSITION], "inline");
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        response.headers()["cross-origin-resource-policy"],
        "same-origin"
    );
    assert_eq!(
        response.headers()["permissions-policy"],
        "accelerometer=(), camera=(), geolocation=(), gyroscope=(), microphone=(), payment=(), usb=()"
    );
    assert_eq!(
        response.headers()[header::ETAG],
        format!("\"{}\"", "00".repeat(32))
    );

    let response = public_response(&object, &Method::HEAD).await;
    assert!(
        response
            .into_body()
            .collect()
            .await
            .expect("HEAD body")
            .to_bytes()
            .is_empty()
    );
}

#[path = "download_io_contract_tests/public_site_failures.rs"]
mod public_site_failures;
