use crate::client::S3Client;
use crate::transport::RequestBody;
use blobyard_contract::StorageError;
use http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct ProviderHead {
    pub(crate) content_length: Option<i64>,
    pub(crate) metadata: HashMap<String, String>,
}

impl S3Client {
    pub(crate) async fn put_object(
        &self,
        key: &str,
        metadata: &HashMap<String, String>,
        length: u64,
        payload_hash: &str,
        body: RequestBody,
    ) -> Result<(), StorageError> {
        let mut headers = metadata_headers(metadata)?;
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
        headers.insert(header::EXPECT, HeaderValue::from_static("100-continue"));
        self.send(Method::PUT, Some(key), &[], headers, body, payload_hash)
            .await
            .map(|_response| ())
    }

    pub(crate) async fn head_object(&self, key: &str) -> Result<ProviderHead, StorageError> {
        let response = self
            .send(
                Method::HEAD,
                Some(key),
                &[],
                HeaderMap::new(),
                RequestBody::Empty,
                Self::empty_hash(),
            )
            .await?;
        let content_length = response
            .headers
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        let metadata = provider_metadata(&response.headers)?;
        Ok(ProviderHead {
            content_length,
            metadata,
        })
    }

    pub(crate) async fn get_object(
        &self,
        key: &str,
        range: Option<&str>,
        path: PathBuf,
    ) -> Result<u64, StorageError> {
        let mut headers = HeaderMap::new();
        if let Some(range) = range {
            insert(&mut headers, header::RANGE, range)?;
        }
        self.send(
            Method::GET,
            Some(key),
            &[],
            headers,
            RequestBody::Empty,
            Self::empty_hash(),
        )
        .await?
        .write_to(path)
        .await
    }

    pub(crate) async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        self.send(
            Method::DELETE,
            Some(key),
            &[],
            HeaderMap::new(),
            RequestBody::Empty,
            Self::empty_hash(),
        )
        .await
        .map(|_response| ())
    }
}

fn provider_metadata(headers: &HeaderMap) -> Result<HashMap<String, String>, StorageError> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            name.as_str()
                .strip_prefix("x-amz-meta-")
                .map(|name| (name, value))
        })
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.to_owned(), value.to_owned()))
                .map_err(|_error| StorageError::IntegrityMismatch)
        })
        .collect()
}

pub(crate) fn metadata_headers(
    metadata: &HashMap<String, String>,
) -> Result<HeaderMap, StorageError> {
    let mut headers = HeaderMap::new();
    for (name, value) in metadata {
        let name = HeaderName::from_bytes(format!("x-amz-meta-{name}").as_bytes())
            .map_err(|_error| StorageError::InvalidInput)?;
        insert(&mut headers, name, value)?;
    }
    Ok(headers)
}

pub(crate) fn insert(
    headers: &mut HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), StorageError> {
    let value = HeaderValue::from_str(value).map_err(|_error| StorageError::InvalidInput)?;
    headers.insert(name, value);
    Ok(())
}
