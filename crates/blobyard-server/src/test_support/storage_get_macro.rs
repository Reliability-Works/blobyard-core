macro_rules! storage_read_methods {
    ($receiver:ident, $error:expr, $head:expr) => {
        fn get(
            &$receiver,
            _key: &StorageKey,
            _range: Option<ByteRange>,
        ) -> Result<StorageRead, StorageError> {
            Err($error)
        }

        fn head(&$receiver, _key: &StorageKey) -> Result<StorageMetadata, StorageError> {
            $head
        }
    };
}

pub(crate) use storage_read_methods;
