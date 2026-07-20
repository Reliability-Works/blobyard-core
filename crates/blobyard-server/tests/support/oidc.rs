#![allow(
    clippy::expect_used,
    clippy::redundant_pub_crate,
    reason = "shared test support is included at both unit and integration module depths"
)]

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

pub(super) struct MockJwksServer {
    address: String,
    requests: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl MockJwksServer {
    pub(super) async fn start(responses: Vec<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock JWKS server");
        let address = listener.local_addr().expect("mock JWKS address");
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let requests = Arc::new(AtomicUsize::new(0));
        let server_requests = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _peer)) = listener.accept().await {
                let mut request = [0_u8; 2_048];
                let _read = stream.read(&mut request).await.expect("read JWKS request");
                server_requests.fetch_add(1, Ordering::SeqCst);
                let response = responses
                    .lock()
                    .expect("mock response queue")
                    .pop_front()
                    .unwrap_or_else(|| response(500, b"missing mock response"));
                stream
                    .write_all(&response)
                    .await
                    .expect("write JWKS response");
            }
        });
        Self {
            address: format!("http://{address}/jwks"),
            requests,
            task,
        }
    }

    pub(super) fn url(&self) -> &str {
        &self.address
    }

    pub(super) fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for MockJwksServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn response(status: u16, body: &[u8]) -> Vec<u8> {
    let reason = if status == 200 { "OK" } else { "Unavailable" };
    let mut value = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    value.extend_from_slice(body);
    value
}

pub(super) fn declared_length_response(length: usize, body: &[u8]) -> Vec<u8> {
    let mut value =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
            .into_bytes();
    value.extend_from_slice(body);
    value
}

pub(super) fn chunked_response(body: &[u8]) -> Vec<u8> {
    let mut value = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",
        body.len()
    )
    .into_bytes();
    value.extend_from_slice(body);
    value.extend_from_slice(b"\r\n0\r\n\r\n");
    value
}
