#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use std::fmt::Write as _;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub async fn capture(socket: &mut tokio::net::TcpStream, read_error: &str) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = socket.read(&mut buffer).await.expect(read_error);
        if count == 0 {
            return request;
        }
        request.extend_from_slice(&buffer[..count]);
        if complete(&request) {
            return request;
        }
    }
}

fn complete(request: &[u8]) -> bool {
    let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
    let body_length = headers.lines().find_map(|line| {
        line.strip_prefix("content-length: ")
            .and_then(|value| value.parse::<usize>().ok())
    });
    body_length.is_none_or(|length| request.len() >= end + 4 + length)
}

pub async fn write_response(
    socket: &mut tokio::net::TcpStream,
    status: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        let _ = write!(response, "{name}: {value}\r\n");
    }
    response.push_str("\r\n");
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.write_all(body).await;
}
