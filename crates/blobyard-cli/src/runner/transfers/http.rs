use blobyard_api_client::SignedHeader;
use blobyard_core::hex_digest;
use blobyard_core::{BlobyardError, ErrorCode, SecretString};
use futures_util::StreamExt;
use indicatif::ProgressBar;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

const STREAM_BUFFER_BYTES: u16 = 60 * 1024;
const PART_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug)]
pub(super) struct SignedTransferClient {
    client: reqwest::Client,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DownloadMeasurements {
    pub(super) size_bytes: u64,
    pub(super) checksum_sha256: String,
}

pub(super) trait DownloadSink: AsyncWrite + Unpin + Send {
    fn sync_all(&self) -> Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>>;
}

impl DownloadSink for tokio::fs::File {
    fn sync_all(&self) -> Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>> {
        Box::pin(Self::sync_all(self))
    }
}

impl SignedTransferClient {
    pub(super) fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub(super) async fn put_file(
        &self,
        url: &SecretString,
        path: &Path,
        size: u64,
        headers: &[SignedHeader],
        progress: &ProgressBar,
    ) -> Result<(), BlobyardError> {
        let body = file_body(path, 0, size, Some(progress.clone())).await?;
        let request = self
            .client
            .put(url.expose_secret())
            .headers(signed_headers(headers)?)
            .header(reqwest::header::CONTENT_LENGTH, size)
            .body(body);
        let response = request.send().await.map_err(network_error)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(storage_response_error(response.status()))
        }
    }

    pub(super) async fn put_part(
        &self,
        url: &SecretString,
        path: &Path,
        offset: u64,
        size: u64,
        progress: &ProgressBar,
    ) -> Result<String, BlobyardError> {
        let mut attempt = 0_u8;
        loop {
            let body = file_body(path, offset, size, None).await?;
            let result = self
                .client
                .put(url.expose_secret())
                .header(reqwest::header::CONTENT_LENGTH, size)
                .body(body)
                .send()
                .await;
            match classify_part_response(result) {
                Ok(etag) => {
                    progress.inc(size);
                    return Ok(etag);
                }
                Err((_error, true)) if attempt + 1 < PART_ATTEMPTS => {
                    tokio::time::sleep(retry_delay(attempt)).await;
                    attempt += 1;
                }
                Err((error, _retryable)) => return Err(error),
            }
        }
    }

    pub(super) async fn download(
        &self,
        url: &SecretString,
        destination: &Path,
        progress: &ProgressBar,
    ) -> Result<DownloadMeasurements, BlobyardError> {
        let response = self.download_response(url).await?;
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .await
            .map_err(write_error)?;
        Self::write_download(response, &mut output, progress).await
    }

    #[cfg(test)]
    pub(super) async fn download_into(
        &self,
        url: &SecretString,
        output: &mut dyn DownloadSink,
        progress: &ProgressBar,
    ) -> Result<DownloadMeasurements, BlobyardError> {
        let response = self.download_response(url).await?;
        Self::write_download(response, output, progress).await
    }

    async fn download_response(
        &self,
        url: &SecretString,
    ) -> Result<reqwest::Response, BlobyardError> {
        let response = self
            .client
            .get(url.expose_secret())
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(storage_response_error(response.status()));
        }
        Ok(response)
    }

    async fn write_download(
        response: reqwest::Response,
        output: &mut dyn DownloadSink,
        progress: &ProgressBar,
    ) -> Result<DownloadMeasurements, BlobyardError> {
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut size_bytes = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(network_error)?;
            output.write_all(&chunk).await.map_err(write_error)?;
            hasher.update(&chunk);
            size_bytes = size_bytes.saturating_add(chunk.len() as u64);
            progress.inc(chunk.len() as u64);
        }
        output.flush().await.map_err(write_error)?;
        output.sync_all().await.map_err(write_error)?;
        Ok(DownloadMeasurements {
            size_bytes,
            checksum_sha256: hex_digest(hasher.finalize().as_slice()),
        })
    }
}

fn classify_part_response(
    result: Result<reqwest::Response, reqwest::Error>,
) -> Result<String, (BlobyardError, bool)> {
    let response = match result {
        Ok(response) => response,
        Err(_error) => return Err((network_error_value(), true)),
    };
    if !response.status().is_success() {
        return Err((
            storage_response_error(response.status()),
            retryable_status(response.status()),
        ));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_etag(value))
        .ok_or_else(|| (storage_error(), false))?;
    Ok(etag.to_owned())
}

pub(super) fn valid_etag(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1_024 && !value.chars().any(char::is_control)
}

pub(super) const fn retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

pub(super) fn retry_delay(attempt: u8) -> Duration {
    let exponent = u32::from(attempt.min(4));
    let base = 100_u64 * (1_u64 << exponent);
    let jitter = uuid::Uuid::new_v4().as_u64_pair().1 % (base / 2 + 1);
    Duration::from_millis(base + jitter)
}

pub(super) async fn file_body(
    path: &Path,
    offset: u64,
    length: u64,
    progress: Option<ProgressBar>,
) -> Result<reqwest::Body, BlobyardError> {
    let mut file = tokio::fs::File::open(path).await.map_err(read_error)?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(read_error)?;
    let stream = futures_util::stream::try_unfold((file, length, progress), |state| async move {
        let (mut file, remaining, progress) = state;
        if remaining == 0 {
            return Ok(None);
        }
        let limited = remaining.min(u64::from(STREAM_BUFFER_BYTES));
        let bytes = limited.to_le_bytes();
        let capacity = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let mut buffer = vec![0_u8; capacity];
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "upload source changed while it was being read",
            ));
        }
        buffer.truncate(count);
        if let Some(progress) = &progress {
            progress.inc(count as u64);
        }
        Ok(Some((buffer, (file, remaining - count as u64, progress))))
    });
    Ok(reqwest::Body::wrap_stream(stream))
}

#[cfg(test)]
pub(super) fn chunk_capacity(value: u64) -> std::io::Result<usize> {
    u16::try_from(value).map(usize::from).map_err(|_error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "upload chunk size is unsupported",
        )
    })
}

fn signed_headers(headers: &[SignedHeader]) -> Result<reqwest::header::HeaderMap, BlobyardError> {
    map_signed_headers(headers, secret_header_value)
}

#[cfg(test)]
pub(super) fn signed_headers_with(
    headers: &[SignedHeader],
    parse_value: fn(&SecretString) -> Result<reqwest::header::HeaderValue, BlobyardError>,
) -> Result<reqwest::header::HeaderMap, BlobyardError> {
    map_signed_headers(headers, parse_value)
}

fn map_signed_headers(
    headers: &[SignedHeader],
    parse_value: fn(&SecretString) -> Result<reqwest::header::HeaderValue, BlobyardError>,
) -> Result<reqwest::header::HeaderMap, BlobyardError> {
    let mut mapped = reqwest::header::HeaderMap::new();
    for header in headers {
        let name = match reqwest::header::HeaderName::from_bytes(header.name.as_bytes()) {
            Ok(name) => name,
            Err(_error) => return Err(storage_error()),
        };
        let value = parse_value(&header.value)?;
        mapped.insert(name, value);
    }
    Ok(mapped)
}

fn secret_header_value(
    value: &SecretString,
) -> Result<reqwest::header::HeaderValue, BlobyardError> {
    header_value(value.expose_secret())
}

pub(super) fn header_value(value: &str) -> Result<reqwest::header::HeaderValue, BlobyardError> {
    reqwest::header::HeaderValue::from_str(value).map_err(|_error| storage_error())
}

fn network_error(_error: reqwest::Error) -> BlobyardError {
    network_error_value()
}

fn network_error_value() -> BlobyardError {
    BlobyardError::from_code(ErrorCode::NetworkError)
}

fn storage_error() -> BlobyardError {
    BlobyardError::from_code(ErrorCode::StorageError)
}

fn storage_response_error(status: reqwest::StatusCode) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        format!(
            "The file transfer was rejected by storage (HTTP {}). Try again.",
            status.as_u16()
        ),
    )
}

fn read_error(_error: std::io::Error) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't read the upload source. Check its permissions and try again.",
    )
}

fn write_error(_error: std::io::Error) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't write the download. Check the destination and try again.",
    )
}
