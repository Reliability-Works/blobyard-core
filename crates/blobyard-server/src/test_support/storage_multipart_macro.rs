macro_rules! storage_multipart_error {
    ($error:expr) => {
        fn begin_multipart(
            &self,
            _key: &StorageKey,
            _expected: &StorageMetadata,
        ) -> Result<MultipartId, StorageError> {
            Err($error)
        }

        fn put_part(
            &self,
            _upload: &MultipartId,
            _number: u32,
            _source: &mut dyn Read,
        ) -> Result<MultipartPart, StorageError> {
            Err($error)
        }

        fn complete_multipart(
            &self,
            _upload: &MultipartId,
            _parts: &[MultipartPart],
        ) -> Result<StorageMetadata, StorageError> {
            Err($error)
        }

        fn abort_multipart(&self, _upload: &MultipartId) -> Result<(), StorageError> {
            Err($error)
        }
    };
}

pub(crate) use storage_multipart_error;
