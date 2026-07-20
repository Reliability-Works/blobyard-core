macro_rules! storage_part_error {
    ($error:expr) => {
        fn put_part(
            &self,
            _upload: &MultipartId,
            _number: u32,
            _source: &mut dyn Read,
        ) -> Result<MultipartPart, StorageError> {
            Err($error)
        }
    };
}

pub(crate) use storage_part_error;
