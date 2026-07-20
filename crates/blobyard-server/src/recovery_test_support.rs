#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{
    ByteRange, MetadataRepository, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage,
    ObjectStorageInventory, StorageError, StorageKey, StorageMetadata, StorageRead,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Mutex;

use crate::storage_multipart_macro::storage_multipart_error;

pub(super) const CONTENT: &[u8] = b"portable recovery bytes";
pub(super) const KEY: &str = "objects/version_recovery";

pub(super) fn installation() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("installation");
    drop(crate::initialize(root.path()).expect("initialize"));
    let repository =
        SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository");
    repository
        .create_project(&blobyard_contract::ProjectRecord {
            id: "project_recovery".to_owned(),
            workspace_id: "workspace_default".to_owned(),
            name: "Recovery".to_owned(),
            slug: Slug::new("recovery").expect("slug"),
        })
        .expect("project");
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("database")
        .execute(
            "INSERT INTO object_versions
             (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms)
             VALUES ('version_recovery', 'project_recovery', 'build.bin', 1,
                     ?1, 'complete', ?2, ?3, 1)",
            rusqlite::params![
                KEY,
                i64::try_from(CONTENT.len()).expect("fixture size"),
                sha256(CONTENT)
            ],
        )
        .expect("version");
    let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
    storage
        .put(&key(KEY), &mut std::io::Cursor::new(CONTENT), None)
        .expect("object");
    root
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}

pub(super) fn key(value: &str) -> StorageKey {
    StorageKey::new(value).expect("storage key")
}

pub(super) fn checksum(bytes: &[u8]) -> ObjectChecksum {
    ObjectChecksum::new(sha256(bytes)).expect("checksum")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GetMode {
    Normal,
    Error(StorageError),
    MetadataMismatch,
    ReaderMismatch,
    ReaderFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PutMode {
    Normal,
    Error(StorageError),
    MetadataMismatch,
}

#[derive(Debug)]
pub(super) struct ScriptedStorage {
    objects: Mutex<BTreeMap<StorageKey, Vec<u8>>>,
    get_mode: GetMode,
    put_mode: PutMode,
    inventory_error: Option<StorageError>,
    inventory_extra: Option<StorageKey>,
    delete_error: Option<StorageError>,
}

impl ScriptedStorage {
    pub(super) fn with_object(bytes: &[u8]) -> Self {
        Self {
            objects: Mutex::new(BTreeMap::from([(key(KEY), bytes.to_vec())])),
            get_mode: GetMode::Normal,
            put_mode: PutMode::Normal,
            inventory_error: None,
            inventory_extra: None,
            delete_error: None,
        }
    }

    pub(super) fn empty() -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            get_mode: GetMode::Normal,
            put_mode: PutMode::Normal,
            inventory_error: None,
            inventory_extra: None,
            delete_error: None,
        }
    }

    pub(super) const fn with_get_mode(mut self, mode: GetMode) -> Self {
        self.get_mode = mode;
        self
    }

    pub(super) const fn with_put_mode(mut self, mode: PutMode) -> Self {
        self.put_mode = mode;
        self
    }

    pub(super) const fn with_inventory_error(mut self, error: StorageError) -> Self {
        self.inventory_error = Some(error);
        self
    }

    pub(super) fn with_inventory_extra(mut self, value: &str) -> Self {
        self.inventory_extra = Some(key(value));
        self
    }

    pub(super) const fn with_delete_error(mut self, error: StorageError) -> Self {
        self.delete_error = Some(error);
        self
    }

    pub(super) fn object_count(&self) -> usize {
        self.objects.lock().expect("objects").len()
    }
}

impl ObjectStorage for ScriptedStorage {
    fn put(
        &self,
        storage_key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        if let PutMode::Error(error) = self.put_mode {
            return Err(error);
        }
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes).expect("scripted source");
        let actual = checksum(&bytes);
        if expected.is_some_and(|value| value != &actual) {
            return Err(StorageError::IntegrityMismatch);
        }
        self.objects
            .lock()
            .expect("objects")
            .insert(storage_key.clone(), bytes.clone());
        if self.put_mode == PutMode::MetadataMismatch {
            Ok(StorageMetadata {
                size: bytes.len() as u64 + 1,
                checksum: actual,
            })
        } else {
            Ok(StorageMetadata {
                size: bytes.len() as u64,
                checksum: actual,
            })
        }
    }

    fn get(
        &self,
        storage_key: &StorageKey,
        _range: Option<ByteRange>,
    ) -> Result<StorageRead, StorageError> {
        if let GetMode::Error(error) = self.get_mode {
            return Err(error);
        }
        let bytes = self
            .objects
            .lock()
            .expect("objects")
            .get(storage_key)
            .cloned()
            .ok_or(StorageError::NotFound)?;
        let metadata = if self.get_mode == GetMode::MetadataMismatch {
            StorageMetadata {
                size: bytes.len() as u64 + 1,
                checksum: checksum(&bytes),
            }
        } else {
            StorageMetadata {
                size: bytes.len() as u64,
                checksum: checksum(&bytes),
            }
        };
        let reader: Box<dyn Read + Send> = match self.get_mode {
            GetMode::ReaderFailure => Box::new(crate::test_support::FailingReader),
            GetMode::ReaderMismatch => Box::new(std::io::Cursor::new(b"tampered".to_vec())),
            GetMode::Normal | GetMode::Error(_) | GetMode::MetadataMismatch => {
                Box::new(std::io::Cursor::new(bytes.clone()))
            }
        };
        Ok(StorageRead {
            reader,
            metadata,
            range: ByteRange {
                start: 0,
                end: bytes.len() as u64,
            },
        })
    }

    fn head(&self, storage_key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let bytes = self
            .objects
            .lock()
            .expect("objects")
            .get(storage_key)
            .cloned()
            .ok_or(StorageError::NotFound)?;
        Ok(StorageMetadata {
            size: bytes.len() as u64,
            checksum: checksum(&bytes),
        })
    }

    fn delete(&self, storage_key: &StorageKey) -> Result<(), StorageError> {
        if let Some(error) = self.delete_error {
            return Err(error);
        }
        self.objects
            .lock()
            .expect("objects")
            .remove(storage_key)
            .map(|_bytes| ())
            .ok_or(StorageError::NotFound)
    }

    storage_multipart_error!(StorageError::Unavailable);
}

impl ObjectStorageInventory for ScriptedStorage {
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError> {
        if let Some(error) = self.inventory_error {
            return Err(error);
        }
        let mut keys: Vec<_> = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .cloned()
            .collect();
        if let Some(extra) = &self.inventory_extra {
            keys.push(extra.clone());
        }
        keys.sort();
        Ok(keys)
    }
}

#[path = "recovery_test_support_tests.rs"]
mod tests;
