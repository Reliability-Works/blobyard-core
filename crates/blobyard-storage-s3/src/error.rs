use blobyard_contract::StorageError;
use http::StatusCode;

pub(crate) fn map_provider_error(status: StatusCode, code: Option<&str>) -> StorageError {
    if let Some(code) = code {
        return map_code(code);
    }
    match status {
        StatusCode::NOT_FOUND => StorageError::NotFound,
        StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => StorageError::Conflict,
        StatusCode::BAD_REQUEST => StorageError::InvalidInput,
        _ => StorageError::Unavailable,
    }
}

fn map_code(code: &str) -> StorageError {
    match code {
        "NoSuchKey" | "NoSuchUpload" | "NotFound" => StorageError::NotFound,
        "ConditionalRequestConflict" | "OperationAborted" | "PreconditionFailed" => {
            StorageError::Conflict
        }
        "EntityTooSmall" | "InvalidArgument" | "InvalidPart" | "InvalidPartOrder"
        | "InvalidRequest" => StorageError::InvalidInput,
        _ => StorageError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{map_code, map_provider_error};
    use blobyard_contract::StorageError;
    use http::StatusCode;

    #[test]
    fn every_provider_code_maps_to_a_stable_contract() {
        assert_codes(
            &["NoSuchKey", "NoSuchUpload", "NotFound"],
            StorageError::NotFound,
        );
        assert_codes(
            &[
                "ConditionalRequestConflict",
                "OperationAborted",
                "PreconditionFailed",
            ],
            StorageError::Conflict,
        );
        assert_codes(
            &[
                "EntityTooSmall",
                "InvalidArgument",
                "InvalidPart",
                "InvalidPartOrder",
                "InvalidRequest",
            ],
            StorageError::InvalidInput,
        );
        assert_eq!(map_code("SlowDown"), StorageError::Unavailable);
    }

    #[test]
    fn every_provider_status_maps_to_a_stable_contract() {
        assert_eq!(
            map_provider_error(StatusCode::NOT_FOUND, None),
            StorageError::NotFound
        );
        assert_eq!(
            map_provider_error(StatusCode::CONFLICT, None),
            StorageError::Conflict
        );
        assert_eq!(
            map_provider_error(StatusCode::PRECONDITION_FAILED, None),
            StorageError::Conflict
        );
        assert_eq!(
            map_provider_error(StatusCode::BAD_REQUEST, None),
            StorageError::InvalidInput
        );
        assert_eq!(
            map_provider_error(StatusCode::SERVICE_UNAVAILABLE, None),
            StorageError::Unavailable
        );
        assert_eq!(
            map_provider_error(StatusCode::SERVICE_UNAVAILABLE, Some("NoSuchKey")),
            StorageError::NotFound
        );
    }

    fn assert_codes(codes: &[&str], expected: StorageError) {
        for code in codes {
            assert_eq!(map_code(code), expected);
        }
    }
}
