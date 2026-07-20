use super::*;

#[test]
fn scripted_storage_covers_its_complete_test_contract() {
    let storage = ScriptedStorage::empty();
    let expected = checksum(CONTENT);
    assert_eq!(
        storage.put(
            &key(KEY),
            &mut std::io::Cursor::new(b"different"),
            Some(&expected),
        ),
        Err(StorageError::IntegrityMismatch)
    );
    assert_eq!(
        storage.get(&key(KEY), None).err(),
        Some(StorageError::NotFound)
    );

    storage
        .put(
            &key(KEY),
            &mut std::io::Cursor::new(CONTENT),
            Some(&expected),
        )
        .expect("put");
    assert_eq!(storage.head(&key(KEY)).expect("head").checksum, expected);
    storage.delete(&key(KEY)).expect("delete");
    assert_eq!(storage.head(&key(KEY)), Err(StorageError::NotFound));
    assert_eq!(storage.delete(&key(KEY)), Err(StorageError::NotFound));

    crate::test_support::assert_multipart_unavailable(&storage);
    assert_eq!(storage.list_object_keys(), Ok(Vec::new()));
}
