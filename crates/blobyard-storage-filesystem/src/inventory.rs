use super::FilesystemStorage;
use blobyard_contract::{ObjectStorageInventory, StorageError, StorageKey};
use std::fs;
use std::path::{Path, PathBuf};

impl ObjectStorageInventory for FilesystemStorage {
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError> {
        let root = self.paths.objects();
        let mut files = Vec::new();
        collect_files(&root, &mut files)?;
        files.sort();
        files
            .into_iter()
            .map(|path| key_from_path(&root, &path))
            .collect()
    }
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), StorageError> {
    let mut directories = vec![directory.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let entries = read_entries(&directory)?;
        for entry in entries {
            collect_entry(entry.path(), entry.file_type(), files, &mut directories)?;
        }
    }
    Ok(())
}

fn read_entries(directory: &Path) -> Result<Vec<fs::DirEntry>, StorageError> {
    let mut entries = fs::read_dir(directory).map_err(unavailable)?;
    collect_entries(&mut entries)
}

fn collect_entries(
    entries: &mut dyn Iterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<Vec<fs::DirEntry>, StorageError> {
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(unavailable)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn collect_entry(
    path: PathBuf,
    kind: std::io::Result<fs::FileType>,
    files: &mut Vec<PathBuf>,
    directories: &mut Vec<PathBuf>,
) -> Result<(), StorageError> {
    let kind = kind.map_err(unavailable)?;
    if kind.is_symlink() {
        return Err(StorageError::InvalidInput);
    }
    if kind.is_dir() {
        directories.push(path);
    } else if kind.is_file() {
        files.push(path);
    } else {
        return Err(StorageError::InvalidInput);
    }
    Ok(())
}

fn unavailable(_error: std::io::Error) -> StorageError {
    StorageError::Unavailable
}

fn key_from_path(root: &Path, path: &Path) -> Result<StorageKey, StorageError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_error| StorageError::InvalidInput)?;
    let value = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or(StorageError::InvalidInput)
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    StorageKey::new(value)
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
