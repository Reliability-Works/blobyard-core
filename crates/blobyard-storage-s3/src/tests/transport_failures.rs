use super::{DownloadTarget, IoFuture, S3Response};
use blobyard_contract::StorageError;
use http::StatusCode;
use std::io;

#[derive(Clone, Copy)]
enum Failure {
    Write,
    Flush,
    Sync,
}

struct FailingTarget(Failure);

impl DownloadTarget for FailingTarget {
    fn write_all(&mut self, _bytes: &[u8]) -> IoFuture<'_> {
        let failure = self.0;
        Box::pin(async move {
            if matches!(failure, Failure::Write) {
                Err(io::Error::other("write failure"))
            } else {
                Ok(())
            }
        })
    }

    fn flush(&mut self) -> IoFuture<'_> {
        let failure = self.0;
        Box::pin(async move {
            if matches!(failure, Failure::Flush) {
                Err(io::Error::other("flush failure"))
            } else {
                Ok(())
            }
        })
    }

    fn sync_all(&mut self) -> IoFuture<'_> {
        let failure = self.0;
        Box::pin(async move {
            if matches!(failure, Failure::Sync) {
                Err(io::Error::other("sync failure"))
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::test]
async fn download_target_maps_write_flush_and_sync_failures() {
    for failure in [Failure::Write, Failure::Flush, Failure::Sync] {
        let response = S3Response::from_items(StatusCode::OK, vec![Ok(b"body".to_vec())]);
        assert_eq!(
            response
                .write_to_target(Box::new(FailingTarget(failure)))
                .await,
            Err(StorageError::Unavailable)
        );
    }
}
