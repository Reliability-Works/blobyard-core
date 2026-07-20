use blobyard_contract::{MultipartId, StorageError, StorageKey};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) struct StoragePaths {
    root: PathBuf,
}

impl StoragePaths {
    pub(super) fn create(root: &Path) -> Result<Self, StorageError> {
        fs::create_dir_all(root).map_err(|_error| StorageError::Unavailable)?;
        reject_symlink(root)?;
        canonicalize_directory(root).and_then(|root| {
            let paths = Self { root };
            for directory in [paths.objects(), paths.metadata_root(), paths.multipart()] {
                fs::create_dir_all(&directory).map_err(|_error| StorageError::Unavailable)?;
                reject_symlink(&directory)?;
            }
            Ok(paths)
        })
    }

    pub(super) fn object(&self, key: &StorageKey) -> PathBuf {
        Self::key_path(&self.objects(), key)
    }

    pub(super) fn metadata(&self, key: &StorageKey) -> PathBuf {
        let mut path = Self::key_path(&self.metadata_root(), key);
        path.set_extension(format!(
            "{}blobyard-meta",
            path.extension()
                .map_or_else(String::new, |value| format!("{}.", value.to_string_lossy()))
        ));
        path
    }

    pub(super) fn upload(&self, id: &MultipartId) -> Result<PathBuf, StorageError> {
        if id.0.is_empty()
            || id.0.len() > 64
            || !id
                .0
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(StorageError::InvalidInput);
        }
        let path = self.multipart().join(&id.0);
        Ok(path)
    }

    pub(super) fn multipart(&self) -> PathBuf {
        self.root.join("multipart")
    }

    pub(super) fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn metadata_root(&self) -> PathBuf {
        self.root.join("metadata")
    }

    fn key_path(base: &Path, key: &StorageKey) -> PathBuf {
        base.join(key.as_str())
    }
}

pub(super) fn canonicalize_directory(path: &Path) -> Result<PathBuf, StorageError> {
    fs::canonicalize(path).map_err(|_error| StorageError::Unavailable)
}

pub(super) fn secure_parent(path: &Path) -> Result<&Path, StorageError> {
    let parent = path.parent().ok_or(StorageError::InvalidInput)?;
    fs::create_dir_all(parent).map_err(|_error| StorageError::Unavailable)?;
    let mut current = Some(parent);
    while let Some(directory) = current {
        reject_symlink(directory)?;
        current = directory.parent();
    }
    Ok(parent)
}

fn reject_symlink(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_error| StorageError::Unavailable)?;
    if metadata.file_type().is_symlink() {
        Err(StorageError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
