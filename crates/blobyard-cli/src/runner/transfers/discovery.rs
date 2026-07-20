use crate::commands::UploadArgs;
use blobyard_core::{BlobyardError, ErrorCode};
use ignore::{DirEntry, WalkBuilder};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UploadFile {
    pub(super) source: PathBuf,
    pub(super) logical_path: String,
    pub(super) filename: String,
}

pub(super) fn discover(arguments: &UploadArgs) -> Result<Vec<UploadFile>, BlobyardError> {
    let metadata = std::fs::symlink_metadata(&arguments.source).map_err(local_read_error)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid_source(
            "The upload source can't be a symbolic link.",
        ));
    }
    if metadata.is_file() {
        return file_entry(&arguments.source, arguments.path.as_deref()).map(|file| vec![file]);
    }
    if !metadata.is_dir() {
        return Err(invalid_source(
            "The upload source must be a regular file or directory.",
        ));
    }
    discover_directory(arguments)
}

fn discover_directory(arguments: &UploadArgs) -> Result<Vec<UploadFile>, BlobyardError> {
    let mut effective = arguments.clone();
    effective.path = Some(directory_prefix(arguments)?);
    let mut builder = WalkBuilder::new(&arguments.source);
    builder
        .hidden(false)
        .follow_links(false)
        .standard_filters(!arguments.include_ignored)
        .add_custom_ignore_filename(".blobyardignore");
    if !arguments.include_ignored {
        builder.filter_entry(include_default);
    }
    let mut files = Vec::new();
    for result in builder.build() {
        let entry = result.map_err(local_read_error)?;
        collect_entry(&effective, &entry, &mut files)?;
    }
    if files.is_empty() {
        return Err(invalid_source(
            "The upload directory doesn't contain any files.",
        ));
    }
    files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    Ok(files)
}

fn directory_prefix(arguments: &UploadArgs) -> Result<String, BlobyardError> {
    directory_prefix_with(arguments, |path| std::fs::canonicalize(path))
}

pub(super) fn directory_prefix_with(
    arguments: &UploadArgs,
    canonicalize: fn(&Path) -> std::io::Result<PathBuf>,
) -> Result<String, BlobyardError> {
    if let Some(path) = &arguments.path {
        return validate_logical(path);
    }
    let canonical = canonicalize(&arguments.source).map_err(local_read_error)?;
    directory_name(&canonical)
}

pub(super) fn directory_name(path: &Path) -> Result<String, BlobyardError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_source("The upload directory name isn't valid UTF-8."))
        .and_then(validate_logical)
}

pub(super) fn collect_entry(
    arguments: &UploadArgs,
    entry: &DirEntry,
    files: &mut Vec<UploadFile>,
) -> Result<(), BlobyardError> {
    if entry.depth() == 0 || entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return Ok(());
    }
    if entry.file_type().is_some_and(|kind| kind.is_symlink()) {
        return Ok(());
    }
    if !entry.file_type().is_some_and(|kind| kind.is_file()) {
        return Err(invalid_source(
            "The upload directory contains an unsupported entry.",
        ));
    }
    files.push(relative_file(arguments, entry.path())?);
    Ok(())
}

pub(super) fn relative_file(
    arguments: &UploadArgs,
    path: &Path,
) -> Result<UploadFile, BlobyardError> {
    let relative = path
        .strip_prefix(&arguments.source)
        .map_err(local_read_error)?;
    let relative = portable_path(relative)?;
    let logical = join_logical(arguments.path.as_deref(), &relative)?;
    file_entry(path, Some(&logical))
}

pub(super) fn file_entry(
    source: &Path,
    logical: Option<&str>,
) -> Result<UploadFile, BlobyardError> {
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid_source("The upload filename isn't valid UTF-8."))?;
    let logical_path = logical.map_or_else(|| Ok(filename.to_owned()), validate_logical)?;
    Ok(UploadFile {
        source: source.to_path_buf(),
        logical_path,
        filename: filename.to_owned(),
    })
}

fn join_logical(prefix: Option<&str>, relative: &str) -> Result<String, BlobyardError> {
    prefix.map_or_else(
        || validate_logical(relative),
        |prefix| validate_logical(&format!("{prefix}/{relative}")),
    )
}

pub(super) fn validate_logical(value: &str) -> Result<String, BlobyardError> {
    let invalid = value.is_empty()
        || value.len() > 2_048
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."));
    if invalid {
        Err(invalid_source("The destination path isn't valid."))
    } else {
        Ok(value.to_owned())
    }
}

pub(super) fn portable_path(path: &Path) -> Result<String, BlobyardError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| invalid_source("An upload path isn't valid UTF-8.")),
            _ => Err(invalid_source("An upload path isn't safe.")),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

fn include_default(entry: &DirEntry) -> bool {
    entry.depth() == 0 || !default_excluded(entry.file_name().to_string_lossy().as_ref())
}

pub(super) fn default_excluded(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | ".next" | "target" | ".cache" | ".turbo" | "__pycache__"
    ) || name.starts_with(".blobyard-resume-")
}

fn invalid_source(message: &'static str) -> BlobyardError {
    BlobyardError::new(ErrorCode::InvalidRequest, message)
}

fn local_read_error<E>(_error: E) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't read the upload source. Check its permissions and try again.",
    )
}
