macro_rules! storage_put_error {
    ($error:expr) => {
        fn put(
            &self,
            _key: &StorageKey,
            _source: &mut dyn Read,
            _expected: Option<&ObjectChecksum>,
        ) -> Result<StorageMetadata, StorageError> {
            Err($error)
        }
    };
}

pub(crate) use storage_put_error;
