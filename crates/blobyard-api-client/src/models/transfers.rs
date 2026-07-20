use blobyard_core::{BlobyardUri, SecretString, Slug};
use serde::{Deserialize, Serialize};

/// Storage strategy selected for an upload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStrategy {
    /// One bounded signed PUT.
    Single,
    /// Multipart transfer with explicit completion.
    Multipart,
}

/// Reserves quota and selects an upload strategy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestUploadRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Normalized logical object path.
    pub path: String,
    /// Original safe filename.
    pub filename: String,
    /// Exact byte length.
    pub size_bytes: u64,
    /// Lowercase hexadecimal SHA-256 digest.
    pub checksum_sha256: String,
    /// Client-observed content type hint.
    pub content_type: String,
    /// Normalized source repository when safely discoverable.
    pub git_repository: Option<String>,
    /// Source commit when safely discoverable.
    pub git_commit: Option<String>,
    /// Source branch when safely discoverable.
    pub git_branch: Option<String>,
}

impl RequestUploadRequest {
    /// Encodes the strict upload reservation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut fields = serde_json::Map::from_iter([
            (
                "workspace".into(),
                serde_json::Value::String(self.workspace.to_string()),
            ),
            (
                "project".into(),
                serde_json::Value::String(self.project.to_string()),
            ),
            ("path".into(), serde_json::Value::String(self.path)),
            ("filename".into(), serde_json::Value::String(self.filename)),
            ("sizeBytes".into(), serde_json::Value::from(self.size_bytes)),
            (
                "checksumSha256".into(),
                serde_json::Value::String(self.checksum_sha256),
            ),
            (
                "contentType".into(),
                serde_json::Value::String(self.content_type),
            ),
        ]);
        insert_optional(&mut fields, "gitRepository", self.git_repository);
        insert_optional(&mut fields, "gitCommit", self.git_commit);
        insert_optional(&mut fields, "gitBranch", self.git_branch);
        serde_json::Value::Object(fields)
    }
}

fn insert_optional(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        fields.insert(name.to_owned(), serde_json::Value::String(value));
    }
}

/// Upload reservation and transfer strategy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUploadResponse {
    /// Stable upload identifier.
    pub upload_id: String,
    /// Selected transfer strategy.
    pub strategy: UploadStrategy,
    /// Signed single-PUT URL when applicable.
    pub upload_url: Option<SecretString>,
    /// Required signed request headers.
    pub headers: Vec<SignedHeader>,
    /// Multipart chunk size when applicable.
    pub part_size_bytes: Option<u64>,
    /// Absolute grant expiry timestamp.
    pub expires_at: String,
}

/// A required header for a signed storage request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedHeader {
    /// Header name.
    pub name: String,
    /// Header value, which may contain signed metadata.
    pub value: SecretString,
}

/// Requests signed URLs for multipart upload parts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestUploadPartsRequest {
    /// Stable upload identifier.
    pub upload_id: String,
    /// Ordered positive part numbers.
    pub part_numbers: Vec<u32>,
}

impl RequestUploadPartsRequest {
    /// Encodes a bounded multipart grant request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "uploadId": self.upload_id,
            "partNumbers": self.part_numbers,
        })
    }
}

/// Signed multipart upload part.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPartGrant {
    /// Positive part number.
    pub part_number: u32,
    /// Short-lived signed PUT URL.
    pub upload_url: SecretString,
}

/// Signed multipart part response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUploadPartsResponse {
    /// Part grants.
    pub parts: Vec<UploadPartGrant>,
    /// Absolute grant expiry timestamp.
    pub expires_at: String,
}

/// A completed multipart part.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletedPart {
    /// Positive part number.
    pub part_number: u32,
    /// Storage-provider entity tag.
    pub etag: String,
}

/// Completes and verifies an upload.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteUploadRequest {
    /// Stable upload identifier.
    pub upload_id: String,
    /// Ordered multipart parts, empty for a single PUT.
    pub parts: Vec<CompletedPart>,
}

impl CompleteUploadRequest {
    /// Encodes verified multipart completion metadata.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "uploadId": self.upload_id, "parts": self.parts })
    }
}

/// Published immutable object version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteUploadResponse {
    /// Canonical immutable object URI.
    pub uri: BlobyardUri,
    /// Verified byte length.
    pub size_bytes: u64,
    /// Verified lowercase hexadecimal SHA-256 digest.
    pub checksum_sha256: String,
}

/// Aborts an incomplete upload.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbortUploadRequest {
    /// Stable upload identifier.
    pub upload_id: String,
}

impl AbortUploadRequest {
    /// Encodes an upload-abort request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "uploadId": self.upload_id })
    }
}

/// Selects an upload for resume/status lookup.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UploadStatusQuery {
    /// Stable upload identifier.
    pub upload_id: String,
}

impl UploadStatusQuery {
    /// Encodes an upload-status query.
    #[must_use]
    pub fn into_query(self) -> String {
        super::encoding::query(&[("uploadId", Some(self.upload_id))])
    }
}

/// Resume metadata for an incomplete upload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadStatusResponse {
    /// Stable server state.
    pub state: String,
    /// Completed multipart part numbers.
    pub completed_parts: Vec<u32>,
}

/// Requests a short-lived signed object download.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestDownloadRequest {
    /// Canonical object URI.
    pub uri: BlobyardUri,
}

impl RequestDownloadRequest {
    /// Encodes a signed-download request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "uri": self.uri })
    }
}

/// Short-lived signed download metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    /// Signed download URL.
    pub download_url: SecretString,
    /// Required response filename.
    pub filename: String,
    /// Expected byte length.
    pub size_bytes: u64,
    /// Expected lowercase hexadecimal SHA-256 digest.
    pub checksum_sha256: String,
    /// Absolute grant expiry timestamp.
    pub expires_at: String,
}
