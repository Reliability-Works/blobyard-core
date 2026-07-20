use super::*;

#[test]
fn multipart_detects_tampered_parts_and_unreadable_keys() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("multipart/tampered.bin").expect("key");
    let upload = storage
        .begin_multipart(&key, &expected_multipart())
        .expect("upload");
    let part = storage
        .put_part(&upload, 1, &mut Cursor::new(b"part"))
        .expect("part");
    let directory = temporary.path().join("multipart").join(&upload.0);
    std::fs::write(directory.join("00001.part"), b"tart").expect("tampered part");
    assert_eq!(
        storage.complete_multipart(&upload, &[part]),
        Err(StorageError::IntegrityMismatch)
    );
    storage
        .abort_multipart(&upload)
        .expect("abort tampered upload");

    for (name, key_error) in [
        ("missing-key.bin", StorageError::NotFound),
        ("directory-key.bin", StorageError::Unavailable),
    ] {
        let key = StorageKey::new(format!("multipart/{name}")).expect("key");
        let upload = storage
            .begin_multipart(&key, &expected_multipart())
            .expect("upload");
        let part = storage
            .put_part(&upload, 1, &mut Cursor::new(b"part"))
            .expect("part");
        let key_path = temporary
            .path()
            .join("multipart")
            .join(&upload.0)
            .join("key");
        std::fs::remove_file(&key_path).expect("remove key");
        if key_error == StorageError::Unavailable {
            std::fs::create_dir(&key_path).expect("directory key");
        }
        assert_eq!(storage.complete_multipart(&upload, &[part]), Err(key_error));
        storage.abort_multipart(&upload).expect("abort upload");
    }
}
