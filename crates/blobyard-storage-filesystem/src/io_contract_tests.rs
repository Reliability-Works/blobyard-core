#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{ObjectStorage, StorageError, StorageKey};
use blobyard_storage_filesystem::{FilesystemStorage, test_seams};
use std::io::{Cursor, Error, Read, Seek, SeekFrom, Write};

#[derive(Debug)]
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture read failure"))
    }
}

#[derive(Debug)]
struct FailingWriter(std::io::ErrorKind);

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(Error::from(self.0))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct FailingFlushWriter;

impl Write for FailingFlushWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(Error::other("fixture flush failure"))
    }
}

#[derive(Debug)]
struct FailingSeeker;

impl Seek for FailingSeeker {
    fn seek(&mut self, _position: SeekFrom) -> std::io::Result<u64> {
        Err(Error::other("fixture seek failure"))
    }
}

#[test]
fn io_failures_map_to_stable_provider_classes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        test_seams::open(&temporary.path().join("missing")),
        Err(StorageError::NotFound)
    );
    assert_eq!(
        test_seams::canonicalize(&temporary.path().join("missing")),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::write(
            &mut FailingWriter(std::io::ErrorKind::PermissionDenied),
            b"payload",
        ),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::write(&mut FailingFlushWriter, b"payload"),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::flush(&mut FailingFlushWriter),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::seek(&mut FailingSeeker, 1),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::add_size(u64::MAX, 1),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        test_seams::copy_from_size(u64::MAX, &mut Cursor::new(b"x"), &mut Vec::new(),),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        test_seams::copy_and_flush(&mut Cursor::new(b"payload"), &mut FailingFlushWriter),
        Err(StorageError::Unavailable)
    );
}

#[test]
fn multipart_helpers_fail_closed_without_committing_bytes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let key = StorageKey::new("multipart/failure.bin").expect("key");
    let key_target = temporary.path().join("key-target");
    std::fs::create_dir(&key_target).expect("key target directory");
    assert_eq!(
        test_seams::write_multipart_key(&key_target, &key),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::remove_multipart_directory(&temporary.path().join("missing")),
        Err(StorageError::Unavailable)
    );

    assert_eq!(
        test_seams::append_multipart_reader(u64::MAX, &mut Cursor::new(b"x"), &mut Vec::new(),),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        test_seams::append_multipart_reader(0, &mut FailingReader, &mut Vec::new()),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::hash_multipart_readers(
            vec![Box::new(Cursor::new(b"part"))],
            &mut FailingFlushWriter,
        ),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        test_seams::hash_multipart_readers(vec![Box::new(FailingReader)], &mut Vec::new()),
        Err(StorageError::Unavailable)
    );

    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    assert_eq!(
        test_seams::put_multipart_readers(&storage, &key, vec![Box::new(FailingReader)]),
        Err(StorageError::Unavailable)
    );
    assert_eq!(storage.head(&key), Err(StorageError::NotFound));
}
