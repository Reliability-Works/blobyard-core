use super::*;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

struct FailingReader;

impl AsyncRead for FailingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("read failed")))
    }
}

impl AsyncBufRead for FailingReader {
    fn poll_fill_buf(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Poll::Ready(Err(io::Error::other("read failed")))
    }

    fn consume(self: Pin<&mut Self>, _amount: usize) {}
}

struct FailingWriter {
    writes_remaining: usize,
    fail_flush: bool,
}

impl AsyncWrite for FailingWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.writes_remaining == 0 {
            return Poll::Ready(Err(io::Error::other("write failed")));
        }
        self.writes_remaining -= 1;
        Poll::Ready(Ok(buffer.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.fail_flush {
            Poll::Ready(Err(io::Error::other("flush failed")))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn serve_propagates_read_write_and_flush_failures() {
    let backend = Backend::success(json!({}));
    assert!(
        serve(FailingReader, tokio::io::sink(), &backend)
            .await
            .is_err()
    );
    let input = request(1, "initialize", json!({ "protocolVersion": "2025-11-25" }));
    for writer in [
        FailingWriter {
            writes_remaining: 0,
            fail_flush: false,
        },
        FailingWriter {
            writes_remaining: 1,
            fail_flush: false,
        },
        FailingWriter {
            writes_remaining: 2,
            fail_flush: true,
        },
    ] {
        assert!(
            serve(BufReader::new(input.as_bytes()), writer, &backend)
                .await
                .is_err()
        );
    }
}
