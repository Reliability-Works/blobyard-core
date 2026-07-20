use crate::client::S3Client;
use crate::client_objects::metadata_headers;
use crate::signing::sha256_hex;
use crate::transport::RequestBody;
use blobyard_contract::{MultipartPart, StorageError};
use http::{HeaderMap, HeaderValue, Method, header};
use std::collections::HashMap;

const XML_BODY_LIMIT: usize = 4 * 1024 * 1024;

impl S3Client {
    pub(crate) async fn create_multipart(
        &self,
        key: &str,
        metadata: &HashMap<String, String>,
    ) -> Result<String, StorageError> {
        let response = self
            .send(
                Method::POST,
                Some(key),
                &[("uploads", "")],
                metadata_headers(metadata)?,
                RequestBody::Empty,
                Self::empty_hash(),
            )
            .await?;
        let body = response.collect(XML_BODY_LIMIT).await?;
        crate::xml::parse_create(&body)
    }

    pub(crate) async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        number: u32,
        length: u64,
        payload_hash: &str,
        body: RequestBody,
    ) -> Result<String, StorageError> {
        let number = number.to_string();
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
        let response = self
            .send(
                Method::PUT,
                Some(key),
                &[("partNumber", &number), ("uploadId", upload_id)],
                headers,
                body,
                payload_hash,
            )
            .await?;
        response
            .headers
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .ok_or(StorageError::Unavailable)
    }

    pub(crate) async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[MultipartPart],
    ) -> Result<(), StorageError> {
        let body = crate::xml::complete_body(parts)?;
        let payload_hash = sha256_hex(&body);
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body.len() as u64));
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml"),
        );
        self.send(
            Method::POST,
            Some(key),
            &[("uploadId", upload_id)],
            headers,
            RequestBody::Bytes(body),
            &payload_hash,
        )
        .await
        .map(|_response| ())
    }

    pub(crate) async fn list_parts(&self, key: &str, upload_id: &str) -> Result<(), StorageError> {
        self.send(
            Method::GET,
            Some(key),
            &[("max-parts", "1"), ("uploadId", upload_id)],
            HeaderMap::new(),
            RequestBody::Empty,
            Self::empty_hash(),
        )
        .await
        .map(|_response| ())
    }

    pub(crate) async fn abort_multipart(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        self.send(
            Method::DELETE,
            Some(key),
            &[("uploadId", upload_id)],
            HeaderMap::new(),
            RequestBody::Empty,
            Self::empty_hash(),
        )
        .await
        .map(|_response| ())
    }
}
