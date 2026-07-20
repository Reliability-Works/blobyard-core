use super::ConfigSource;
use blobyard_core::{BlobyardError, ErrorCode, Slug};

pub(super) fn choose(
    flag: Option<String>,
    environment: Option<String>,
    project: Option<String>,
    fallback: Option<(String, ConfigSource)>,
) -> Option<(String, ConfigSource)> {
    flag.map(|value| (value, ConfigSource::Flag))
        .or_else(|| environment.map(|value| (value, ConfigSource::Environment)))
        .or_else(|| project.map(|value| (value, ConfigSource::Project)))
        .or(fallback)
}

pub(super) const fn identity<T>(value: T) -> T {
    value
}

pub(super) fn resolve_slug(
    field: &str,
    selected: Option<(String, ConfigSource)>,
) -> Result<(Option<Slug>, Option<ConfigSource>), BlobyardError> {
    selected.map_or(Ok((None, None)), |(value, source)| {
        Slug::new(value).map_or_else(
            |_| {
                Err(BlobyardError::new(
                    ErrorCode::InvalidRequest,
                    format!(
                        "The configured {field} slug isn't valid. Use letters, numbers, '-' or '_'."
                    ),
                ))
            },
            |slug| Ok((Some(slug), Some(source))),
        )
    })
}
