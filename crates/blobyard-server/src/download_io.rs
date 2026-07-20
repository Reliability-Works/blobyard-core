use crate::{api::AppState, error::ApiError};
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode, header},
};
use blobyard_contract::{ByteRange, StorageError, StorageKey, StoredObjectRecord};
use futures_util::stream;
use std::io::Read;

pub(crate) async fn response(
    state: &AppState,
    object: &StoredObjectRecord,
    headers: &HeaderMap,
) -> Result<Response<Body>, ApiError> {
    response_from_storage(state.storage.clone(), object, headers).await
}

pub(crate) async fn public_site_response(
    state: &AppState,
    object: &StoredObjectRecord,
    headers: &HeaderMap,
    method: &Method,
) -> Result<Response<Body>, ApiError> {
    public_site_response_with_status(state, object, headers, method, StatusCode::OK).await
}

pub(crate) async fn public_site_response_with_status(
    state: &AppState,
    object: &StoredObjectRecord,
    headers: &HeaderMap,
    method: &Method,
    status: StatusCode,
) -> Result<Response<Body>, ApiError> {
    public_site_response_from_storage(state.storage.clone(), object, headers, method, status).await
}

async fn public_site_response_from_storage(
    storage: std::sync::Arc<dyn blobyard_contract::ObjectStorage>,
    object: &StoredObjectRecord,
    headers: &HeaderMap,
    method: &Method,
    status: StatusCode,
) -> Result<Response<Body>, ApiError> {
    let mut response = response_from_storage(storage, object, headers).await?;
    let content_type =
        HeaderValue::from_str(&object.content_type).map_err(|_error| ApiError::internal())?;
    let checksum = object
        .version
        .checksum
        .as_deref()
        .ok_or_else(ApiError::internal)?;
    let etag =
        HeaderValue::from_str(&format!("\"{checksum}\"")).map_err(|_error| ApiError::internal())?;
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, content_type);
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    response_headers.insert(header::ETAG, etag);
    response_headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response_headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    response_headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(
            "accelerometer=(), camera=(), geolocation=(), gyroscope=(), microphone=(), payment=(), usb=()",
        ),
    );
    if method == Method::HEAD {
        *response.body_mut() = Body::empty();
    }
    if status == StatusCode::NOT_FOUND {
        *response.status_mut() = status;
    }
    Ok(response)
}

async fn response_from_storage(
    storage: std::sync::Arc<dyn blobyard_contract::ObjectStorage>,
    object: &StoredObjectRecord,
    headers: &HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let key = ApiError::internal_result(StorageKey::new(object.version.storage_key.clone()))?;
    let metadata = ApiError::internal_result(
        tokio::task::spawn_blocking({
            let key = key.clone();
            let storage = storage.clone();
            move || storage.head(&key)
        })
        .await,
    )?
    .map_err(download_storage_error)?;
    let range = parse_range(headers.get(header::RANGE), metadata.size)?;
    let read = ApiError::internal_result(
        tokio::task::spawn_blocking(move || storage.get(&key, range)).await,
    )?
    .map_err(download_storage_error)?;
    let status = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    build_response(read, status)
}

/// Test-only entry points for exercising adapter failures in the normal library build.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub mod test_seams {
    use axum::{
        body::Body,
        http::{HeaderMap, Method, Response},
        response::IntoResponse,
    };
    use blobyard_contract::{ObjectStorage, StoredObjectRecord};
    use std::sync::Arc;

    /// Builds a download response with explicit storage authority.
    pub async fn response(
        storage: Arc<dyn ObjectStorage>,
        object: &StoredObjectRecord,
        headers: &HeaderMap,
    ) -> Response<Body> {
        match super::response_from_storage(storage, object, headers).await {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }

    /// Builds an isolated public-site response with explicit storage authority.
    pub async fn public_site_response(
        storage: Arc<dyn ObjectStorage>,
        object: &StoredObjectRecord,
        headers: &HeaderMap,
        method: &Method,
    ) -> Response<Body> {
        match super::public_site_response_from_storage(
            storage,
            object,
            headers,
            method,
            axum::http::StatusCode::OK,
        )
        .await
        {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }
}

const fn download_storage_error(error: StorageError) -> ApiError {
    match error {
        StorageError::NotFound => ApiError::not_found(),
        StorageError::Conflict
        | StorageError::InvalidInput
        | StorageError::IntegrityMismatch
        | StorageError::Unavailable => ApiError::internal(),
    }
}

fn build_response(
    read: blobyard_contract::StorageRead,
    status: StatusCode,
) -> Result<Response<Body>, ApiError> {
    let total_size = read.metadata.size;
    let returned_size = read.range.end - read.range.start;
    let content_range = (status == StatusCode::PARTIAL_CONTENT).then(|| {
        format!(
            "bytes {}-{}/{total_size}",
            read.range.start,
            read.range.end.saturating_sub(1)
        )
    });
    let body = stream_reader(read.reader);
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, returned_size.to_string());
    if let Some(value) = content_range {
        response = response.header(header::CONTENT_RANGE, value);
    }
    ApiError::internal_result(response.body(body))
}

fn stream_reader(mut reader: Box<dyn Read + Send>) -> Body {
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    tokio::task::spawn_blocking(move || {
        let mut buffer = vec![0_u8; 64 * 1_024];
        loop {
            if !forward_chunk(&sender, next_chunk(reader.as_mut(), &mut buffer)) {
                break;
            }
        }
    });
    Body::from_stream(stream::unfold(receiver, |mut receiver| async {
        receiver.recv().await.map(|chunk| (chunk, receiver))
    }))
}

fn forward_chunk(
    sender: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    chunk: Result<Option<Bytes>, std::io::Error>,
) -> bool {
    match chunk {
        Ok(Some(chunk)) => sender.blocking_send(Ok(chunk)).is_ok(),
        Ok(None) => false,
        Err(error) => {
            let _ignored = sender.blocking_send(Err(error));
            false
        }
    }
}

fn next_chunk(reader: &mut dyn Read, buffer: &mut [u8]) -> Result<Option<Bytes>, std::io::Error> {
    let size = reader.read(buffer)?;
    Ok((size != 0).then(|| Bytes::copy_from_slice(&buffer[..size])))
}

fn parse_range(value: Option<&HeaderValue>, size: u64) -> Result<Option<ByteRange>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_error| ApiError::invalid_request())?
        .strip_prefix("bytes=")
        .ok_or_else(ApiError::invalid_request)?;
    if value.contains(',') || size == 0 {
        return Err(ApiError::range_not_satisfiable());
    }
    let (start, end) = value
        .split_once('-')
        .ok_or_else(ApiError::invalid_request)?;
    let range = if start.is_empty() {
        let suffix = parse_number(end)?;
        if suffix == 0 {
            return Err(ApiError::range_not_satisfiable());
        }
        ByteRange::new(size.saturating_sub(suffix.min(size)), size)
    } else {
        let start = parse_number(start)?;
        if start >= size {
            return Err(ApiError::range_not_satisfiable());
        }
        let end = if end.is_empty() {
            size
        } else {
            parse_number(end)?
                .checked_add(1)
                .ok_or_else(ApiError::invalid_request)?
                .min(size)
        };
        ByteRange::new(start, end)
    };
    range
        .map(Some)
        .map_err(|_error| ApiError::range_not_satisfiable())
}

fn parse_number(value: &str) -> Result<u64, ApiError> {
    value.parse().map_err(|_error| ApiError::invalid_request())
}

#[cfg(test)]
#[path = "download_io_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "download_io_contract_tests.rs"]
mod contract_tests;
