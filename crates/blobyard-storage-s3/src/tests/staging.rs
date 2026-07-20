use blobyard_contract::StorageError;
use http::{HeaderMap, StatusCode};
use std::io::{Cursor, Error, Read, Seek, SeekFrom, Write};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture read failure"))
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture write failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(Error::other("fixture flush failure"))
    }
}

struct FailingSeeker;

impl Seek for FailingSeeker {
    fn seek(&mut self, _position: SeekFrom) -> std::io::Result<u64> {
        Err(Error::other("fixture seek failure"))
    }
}

#[test]
fn synchronous_staging_failures_map_to_stable_errors() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let missing = temporary.path().join("missing");
    assert_eq!(
        crate::S3Storage::stage_upload(&missing, &mut Cursor::new(b"bytes")).err(),
        Some(StorageError::Unavailable)
    );
    assert_eq!(
        crate::S3Storage::empty_download(&missing).err(),
        Some(StorageError::Unavailable)
    );
    assert_eq!(
        crate::S3Storage::hash_path(&missing),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        super::copy_and_hash_from_size(&mut FailingReader, &mut Vec::new(), 0),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        super::copy_and_hash_from_size(&mut Cursor::new(b"x"), &mut FailingWriter, 0),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        super::copy_and_hash_from_size(&mut Cursor::new(b"x"), &mut Vec::new(), u64::MAX,),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        super::flush_writer(&mut FailingWriter),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        super::rewind_reader(&mut FailingSeeker),
        Err(StorageError::Unavailable)
    );
    Ok(())
}

#[test]
fn staged_reader_reopen_failure_is_stable() -> TestResult {
    let temporary = tempfile::NamedTempFile::new()?;
    std::fs::remove_file(temporary.path())?;
    assert_eq!(
        crate::StagedRead::open(temporary).err(),
        Some(StorageError::Unavailable)
    );
    Ok(())
}

#[test]
fn asynchronous_download_failures_are_stable() -> TestResult {
    let runtime = crate::RuntimeBridge::start()?;
    let temporary = tempfile::tempdir()?;
    let missing_target = temporary.path().join("missing").join("object");
    assert_eq!(
        runtime.run(
            crate::transport::S3Response::from_bytes(
                StatusCode::OK,
                HeaderMap::new(),
                b"bytes".to_vec(),
            )
            .write_to(missing_target),
        ),
        Err(StorageError::Unavailable)
    );

    let target = temporary.path().join("copy-failure");
    assert_eq!(
        runtime.run(
            crate::transport::S3Response::from_items(
                StatusCode::OK,
                vec![Err(StorageError::Unavailable)],
            )
            .write_to(target),
        ),
        Err(StorageError::Unavailable)
    );
    Ok(())
}
