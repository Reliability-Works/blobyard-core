use blobyard_contract::StoredObjectRecord;
use percent_encoding::percent_decode_str;
use std::collections::HashSet;

const MAXIMUM_SITE_FILES: usize = 10_000;

pub(crate) struct SiteManifestObject {
    pub(crate) normalized_path: String,
    pub(crate) version_id: String,
    pub(crate) byte_size: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) enum SiteManifestError {
    Invalid,
    Duplicate,
}

pub(crate) fn snapshot_manifest(
    root: &str,
    objects: Vec<StoredObjectRecord>,
    valid_path: fn(&str) -> bool,
) -> Result<Vec<SiteManifestObject>, SiteManifestError> {
    if objects.is_empty() || objects.len() > MAXIMUM_SITE_FILES {
        return Err(SiteManifestError::Invalid);
    }
    let mut paths = HashSet::with_capacity(objects.len());
    let mut files = Vec::with_capacity(objects.len());
    for object in objects {
        let path = object
            .version
            .object_path
            .strip_prefix(root)
            .ok_or(SiteManifestError::Invalid)?;
        if !valid_path(path) {
            return Err(SiteManifestError::Invalid);
        }
        if !paths.insert(path.to_owned()) {
            return Err(SiteManifestError::Duplicate);
        }
        files.push(SiteManifestObject {
            normalized_path: path.to_owned(),
            version_id: object.version.id,
            byte_size: object.version.size,
        });
    }
    if !paths.contains("index.html") {
        return Err(SiteManifestError::Invalid);
    }
    files.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
    Ok(files)
}

#[derive(Clone, Copy)]
pub(crate) enum PublicPathError {
    InvalidPath,
    NotFound,
}

pub(crate) fn public_path(
    path: &str,
    root_path: &str,
    directory_suffix: &str,
    valid_path: fn(&str) -> bool,
) -> Result<String, PublicPathError> {
    if !path.starts_with('/') || path.contains("//") || path.contains(['\\', '?', '#']) {
        return Err(PublicPathError::NotFound);
    }
    let directory = path.ends_with('/');
    let raw = if directory {
        path.trim_matches('/')
    } else {
        path.trim_start_matches('/')
    };
    if raw.is_empty() {
        return Ok(root_path.to_owned());
    }
    let decoded = raw
        .split('/')
        .map(decode_segment)
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    let normalized = if directory {
        format!("{decoded}{directory_suffix}")
    } else {
        decoded
    };
    if valid_path(&normalized) {
        Ok(normalized)
    } else {
        Err(PublicPathError::InvalidPath)
    }
}

fn decode_segment(value: &str) -> Result<String, PublicPathError> {
    let decoded = percent_decode_str(value)
        .decode_utf8()
        .map_err(|_error| PublicPathError::NotFound)?;
    if decoded.is_empty()
        || matches!(decoded.as_ref(), "." | "..")
        || decoded.contains(['/', '\\'])
        || decoded.chars().any(char::is_control)
    {
        return Err(PublicPathError::NotFound);
    }
    Ok(decoded.into_owned())
}
