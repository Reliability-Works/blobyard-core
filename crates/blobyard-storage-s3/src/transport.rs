use blobyard_contract::StorageError;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use url::Url;

pub(crate) type TransportFuture =
    Pin<Box<dyn Future<Output = Result<S3Response, StorageError>> + Send>>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>;
type IoFuture<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

trait DownloadTarget: Send {
    fn write_all<'a>(&'a mut self, bytes: &'a [u8]) -> IoFuture<'a>;
    fn flush(&mut self) -> IoFuture<'_>;
    fn sync_all(&mut self) -> IoFuture<'_>;
}

struct FileTarget(tokio::fs::File);

impl DownloadTarget for FileTarget {
    fn write_all<'a>(&'a mut self, bytes: &'a [u8]) -> IoFuture<'a> {
        Box::pin(async move { tokio::io::AsyncWriteExt::write_all(&mut self.0, bytes).await })
    }

    fn flush(&mut self) -> IoFuture<'_> {
        Box::pin(async move { tokio::io::AsyncWriteExt::flush(&mut self.0).await })
    }

    fn sync_all(&mut self) -> IoFuture<'_> {
        Box::pin(async move { self.0.sync_all().await })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RequestBody {
    Empty,
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub(crate) struct S3Request {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: RequestBody,
}

pub(crate) struct S3Response {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    body: ResponseStream,
}

pub(crate) trait S3Transport: Send + Sync {
    fn send(&self, request: S3Request) -> TransportFuture;
}

#[derive(Clone, Debug)]
pub(crate) struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub(crate) fn new() -> Result<Self, StorageError> {
        reqwest::Client::builder()
            .build()
            .map(|client| Self { client })
            .map_err(|_error| StorageError::Unavailable)
    }
}

impl S3Transport for ReqwestTransport {
    fn send(&self, request: S3Request) -> TransportFuture {
        let client = self.client.clone();
        Box::pin(async move {
            let body = request.body.into_reqwest().await?;
            let response = client
                .request(request.method, request.url)
                .headers(request.headers)
                .body(body)
                .send()
                .await
                .map_err(|_error| StorageError::Unavailable)?;
            Ok(S3Response::from_reqwest(response))
        })
    }
}

impl RequestBody {
    async fn into_reqwest(self) -> Result<reqwest::Body, StorageError> {
        match self {
            Self::Empty => Ok(reqwest::Body::default()),
            Self::Bytes(bytes) => Ok(reqwest::Body::from(bytes)),
            Self::File(path) => tokio::fs::File::open(path)
                .await
                .map(reqwest::Body::from)
                .map_err(|_error| StorageError::Unavailable),
        }
    }

    #[cfg(test)]
    pub(crate) async fn into_bytes(self) -> Result<Vec<u8>, StorageError> {
        match self {
            Self::Empty => Ok(Vec::new()),
            Self::Bytes(bytes) => Ok(bytes),
            Self::File(path) => tokio::fs::read(path)
                .await
                .map_err(|_error| StorageError::Unavailable),
        }
    }
}

impl S3Response {
    fn from_reqwest(response: reqwest::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let stream = response
            .bytes_stream()
            .map(|item| item.map_err(|_error| StorageError::Unavailable));
        Self {
            status,
            headers,
            body: Box::pin(stream),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(status: StatusCode, headers: HeaderMap, body: Vec<u8>) -> Self {
        let stream = futures_util::stream::once(async move { Ok(Bytes::from(body)) });
        Self {
            status,
            headers,
            body: Box::pin(stream),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_items(
        status: StatusCode,
        items: Vec<Result<Vec<u8>, StorageError>>,
    ) -> Self {
        let stream =
            futures_util::stream::iter(items.into_iter().map(|item| item.map(Bytes::from)));
        Self {
            status,
            headers: HeaderMap::new(),
            body: Box::pin(stream),
        }
    }

    pub(crate) async fn collect(mut self, limit: usize) -> Result<Vec<u8>, StorageError> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.body.next().await {
            let chunk = chunk?;
            let next = bytes.len().saturating_add(chunk.len());
            if next > limit {
                return Err(StorageError::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub(crate) async fn write_to(self, path: PathBuf) -> Result<u64, StorageError> {
        let target = tokio::fs::File::create(path)
            .await
            .map_err(|_error| StorageError::Unavailable)?;
        self.write_to_target(Box::new(FileTarget(target))).await
    }

    async fn write_to_target(
        mut self,
        mut target: Box<dyn DownloadTarget>,
    ) -> Result<u64, StorageError> {
        let mut count = 0_u64;
        while let Some(chunk) = self.body.next().await {
            let chunk = chunk?;
            target
                .write_all(&chunk)
                .await
                .map_err(|_error| StorageError::Unavailable)?;
            count = count.saturating_add(chunk.len() as u64);
        }
        target
            .flush()
            .await
            .map_err(|_error| StorageError::Unavailable)?;
        target
            .sync_all()
            .await
            .map_err(|_error| StorageError::Unavailable)?;
        Ok(count)
    }
}

#[cfg(test)]
#[path = "tests/transport_failures.rs"]
mod failure_tests;
