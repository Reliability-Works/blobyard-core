use crate::error::ApiError;
use blobyard_api_client::ObjectSource;

pub(super) const fn object_source(source: blobyard_contract::ObjectSource) -> ObjectSource {
    match source {
        blobyard_contract::ObjectSource::Ci => ObjectSource::Ci,
        blobyard_contract::ObjectSource::Cli => ObjectSource::Cli,
        blobyard_contract::ObjectSource::Inbox => ObjectSource::Inbox,
        blobyard_contract::ObjectSource::Preview => ObjectSource::Preview,
        blobyard_contract::ObjectSource::Web => ObjectSource::Web,
    }
}

pub(super) fn validate_prefix(prefix: Option<&str>) -> Result<(), ApiError> {
    let Some(prefix) = prefix else {
        return Ok(());
    };
    let valid = !prefix.is_empty()
        && prefix.len() <= 512
        && !prefix.starts_with('/')
        && !prefix.contains('\\')
        && !prefix.chars().any(char::is_control)
        && prefix
            .trim_end_matches('/')
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."));
    if valid {
        Ok(())
    } else {
        Err(ApiError::invalid_request())
    }
}
