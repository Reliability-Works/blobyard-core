use crate::S3Credentials;
use crate::signing::sign_headers;
use crate::transport::{RequestBody, S3Request, S3Response, S3Transport};
use blobyard_contract::StorageError;
use http::{HeaderMap, Method};
use std::sync::Arc;
use time::OffsetDateTime;
use url::Url;

const ERROR_BODY_LIMIT: usize = 64 * 1024;
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Clone)]
pub(crate) struct S3Client {
    transport: Arc<dyn S3Transport>,
    endpoint: Url,
    region: String,
    bucket: String,
    credentials: S3Credentials,
    force_path_style: bool,
}

impl S3Client {
    pub(crate) fn new(
        transport: Arc<dyn S3Transport>,
        endpoint: Url,
        region: String,
        bucket: String,
        credentials: S3Credentials,
        force_path_style: bool,
    ) -> Self {
        Self {
            transport,
            endpoint,
            region,
            bucket,
            credentials,
            force_path_style,
        }
    }

    pub(crate) async fn send(
        &self,
        method: Method,
        key: Option<&str>,
        query: &[(&str, &str)],
        mut headers: HeaderMap,
        body: RequestBody,
        payload_hash: &str,
    ) -> Result<S3Response, StorageError> {
        let mut url = self.request_url(key)?;
        set_query(&mut url, query);
        sign_headers(
            &method,
            &url,
            &mut headers,
            payload_hash,
            &self.region,
            &self.credentials,
            OffsetDateTime::now_utc(),
        )?;
        let request = S3Request {
            method,
            url,
            headers,
            body,
        };
        let response = self.transport.send(request).await?;
        ensure_success(response).await
    }

    pub(crate) const fn empty_hash() -> &'static str {
        EMPTY_SHA256
    }

    fn request_url(&self, key: Option<&str>) -> Result<Url, StorageError> {
        let mut url = self.endpoint.clone();
        let mut segments = Vec::new();
        if self.force_path_style {
            segments.push(self.bucket.as_str());
        } else {
            let host = url.host_str().ok_or(StorageError::InvalidInput)?;
            let bucket_host = format!("{}.{host}", self.bucket);
            url.set_host(Some(&bucket_host))
                .map_err(|_error| StorageError::InvalidInput)?;
        }
        if let Some(key) = key {
            segments.extend(key.split('/'));
        }
        if !segments.is_empty() {
            append_segments(&mut url, &segments)?;
        }
        Ok(url)
    }
}

async fn ensure_success(response: S3Response) -> Result<S3Response, StorageError> {
    if response.status.is_success() {
        return Ok(response);
    }
    let status = response.status;
    let body = response.collect(ERROR_BODY_LIMIT).await?;
    let code = crate::xml::parse_error_code(&body);
    Err(crate::error::map_provider_error(status, code.as_deref()))
}

fn append_segments(url: &mut Url, segments: &[&str]) -> Result<(), StorageError> {
    let mut path = url
        .path_segments_mut()
        .map_err(|()| StorageError::InvalidInput)?;
    for segment in segments {
        path.push(segment);
    }
    Ok(())
}

fn set_query(url: &mut Url, query: &[(&str, &str)]) {
    if query.is_empty() {
        url.set_query(None);
        return;
    }
    let mut pairs = query
        .iter()
        .map(|(name, value)| (aws_encode(name), aws_encode(value)))
        .collect::<Vec<_>>();
    pairs.sort();
    let value = pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    url.set_query(Some(&value));
}

fn aws_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(*byte));
            }
            other => {
                const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(char::from(DIGITS[usize::from(other >> 4)]));
                encoded.push(char::from(DIGITS[usize::from(other & 0x0f)]));
            }
        }
    }
    encoded
}
