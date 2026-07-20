#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::http::{DownloadSink, SignedTransferClient, file_body};
use blobyard_core::{ErrorCode, SecretString};
use http_body_util::BodyExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SinkFailure {
    Write,
    Flush,
    Sync,
}

struct FaultySink(SinkFailure);

impl tokio::io::AsyncWrite for FaultySink {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.0 == SinkFailure::Write {
            Poll::Ready(Err(std::io::Error::other("synthetic write failure")))
        } else {
            Poll::Ready(Ok(buffer.len()))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.0 == SinkFailure::Flush {
            Poll::Ready(Err(std::io::Error::other("synthetic flush failure")))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl DownloadSink for FaultySink {
    fn sync_all(&self) -> Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>> {
        let failure = self.0 == SinkFailure::Sync;
        Box::pin(async move {
            if failure {
                Err(std::io::Error::other("synthetic sync failure"))
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::test]
async fn signed_transfer_maps_stream_and_sink_failures() {
    for failure in [SinkFailure::Write, SinkFailure::Flush, SinkFailure::Sync] {
        let (url, task) = serve_bytes().await;
        let secret = SecretString::new(url).expect("url");
        let mut sink = FaultySink(failure);
        let error = SignedTransferClient::new()
            .download_into(&secret, &mut sink, &indicatif::ProgressBar::hidden())
            .await
            .expect_err("sink failure");
        assert_eq!(error.code(), ErrorCode::StorageError);
        task.await.expect("server");
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind dead");
    let dead_url = format!("http://{}/dead", listener.local_addr().expect("address"));
    drop(listener);
    let mut sink = FaultySink(SinkFailure::Sync);
    let dead = SecretString::new(dead_url).expect("dead");
    let network = SignedTransferClient::new()
        .download_into(&dead, &mut sink, &indicatif::ProgressBar::hidden())
        .await
        .expect_err("network");
    assert_eq!(network.code(), ErrorCode::NetworkError);

    let (url, task) = serve_response(10, b"short").await;
    let truncated = SecretString::new(url).expect("truncated url");
    let stream_error = SignedTransferClient::new()
        .download_into(&truncated, &mut sink, &indicatif::ProgressBar::hidden())
        .await
        .expect_err("truncated response");
    assert_eq!(stream_error.code(), ErrorCode::NetworkError);
    task.await.expect("truncated server");

    #[cfg(unix)]
    {
        let temp = tempfile::tempdir().expect("temp");
        let body = file_body(temp.path(), 0, 1, None)
            .await
            .expect("directory opens on Unix");
        assert!(body.collect().await.is_err());
        assert_seek_failure(temp.path()).await;
    }
}

async fn serve_bytes() -> (String, tokio::task::JoinHandle<()>) {
    serve_response(5, b"bytes").await
}

async fn serve_response(
    declared_length: usize,
    body: &'static [u8],
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 4_096];
        let _ = socket.read(&mut request).await;
        let header = format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {declared_length}\r\n\r\n"
        );
        socket.write_all(header.as_bytes()).await.expect("header");
        socket.write_all(body).await.expect("body");
    });
    (format!("http://{address}/object"), task)
}

#[cfg(unix)]
async fn assert_seek_failure(root: &std::path::Path) {
    use std::io::Write as _;
    let fifo = root.join("upload.fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo");
    assert!(status.success());
    let writer_path = fifo.clone();
    let writer = tokio::task::spawn_blocking(move || {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .open(writer_path)
            .expect("fifo writer");
        let _ = output.write_all(b"x");
    });
    assert!(file_body(&fifo, 1, 1, None).await.is_err());
    writer.await.expect("writer");
}
