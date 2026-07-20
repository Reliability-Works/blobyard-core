use crate::error::ApiError;

pub(super) const fn reject_cursor(cursor: Option<&str>) -> Result<(), ApiError> {
    if cursor.is_some() {
        Err(ApiError::invalid_request())
    } else {
        Ok(())
    }
}
