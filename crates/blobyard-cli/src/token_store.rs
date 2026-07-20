use blobyard_core::{BlobyardError, ErrorCode, SecretString, Slug};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Warning shown whenever the explicit plaintext file fallback is selected.
pub const FILE_FALLBACK_WARNING: &str =
    "Warning: platform credential storage is unavailable; using a 0600 credential file.";

/// Storage for the single persisted CLI refresh token.
pub trait TokenStore: Send + Sync {
    /// Loads the refresh token, when present.
    ///
    /// # Errors
    ///
    /// Returns a safe local storage error when the credential cannot be read.
    fn load(&self) -> Result<Option<SecretString>, BlobyardError>;

    /// Atomically stores a replacement refresh token.
    ///
    /// # Errors
    ///
    /// Returns a safe local storage error when persistence fails.
    fn save(&self, token: &SecretString) -> Result<(), BlobyardError>;

    /// Deletes any persisted refresh token.
    ///
    /// # Errors
    ///
    /// Returns a safe local storage error when deletion fails.
    fn delete(&self) -> Result<(), BlobyardError>;
}

/// Platform credential store backed by Keychain, Secret Service, or Credential Manager.
pub struct PlatformTokenStore {
    entry: keyring::Entry,
}

impl PlatformTokenStore {
    /// Opens Blobyard's platform-native refresh-token entry.
    ///
    /// # Errors
    ///
    /// Returns a safe local storage error when no platform store can be initialized.
    pub fn new() -> Result<Self, BlobyardError> {
        Self::from_result(keyring::Entry::new("com.blobyard.cli", "refresh-token"))
    }

    /// Opens an isolated platform-native entry for a validated profile.
    ///
    /// # Errors
    ///
    /// Returns a safe local storage error when no platform store can be initialized.
    pub fn for_profile(profile: &Slug) -> Result<Self, BlobyardError> {
        Self::from_result(keyring::Entry::new(
            "com.blobyard.cli",
            &profile_account(profile),
        ))
    }

    fn from_result(entry: keyring::Result<keyring::Entry>) -> Result<Self, BlobyardError> {
        entry
            .map(|entry| Self { entry })
            .map_err(|_| credential_error())
    }
}

fn profile_account(profile: &Slug) -> String {
    if profile.as_str() == "cloud" {
        "refresh-token".to_owned()
    } else {
        format!("refresh-token:{}", profile.as_str())
    }
}

impl std::fmt::Debug for PlatformTokenStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformTokenStore")
            .finish_non_exhaustive()
    }
}

impl TokenStore for PlatformTokenStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        match self.entry.get_secret() {
            Ok(bytes) => String::from_utf8(bytes)
                .map_err(|_| credential_error())
                .and_then(SecretString::new)
                .map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(credential_error()),
        }
    }

    fn save(&self, token: &SecretString) -> Result<(), BlobyardError> {
        self.entry
            .set_secret(token.expose_secret().as_bytes())
            .map_err(|_| credential_error())
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(credential_error()),
        }
    }
}

/// Explicit atomic credential-file fallback.
#[derive(Clone, Debug)]
pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    /// Creates a fallback store at an explicit dedicated path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the credential path for diagnostics without token contents.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self) -> Result<Option<SecretString>, BlobyardError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(credential_error()),
        };
        ensure_private(&self.path)?;
        String::from_utf8(bytes)
            .map_err(|_| credential_error())
            .and_then(SecretString::new)
            .map(Some)
    }

    fn save(&self, token: &SecretString) -> Result<(), BlobyardError> {
        let parent = self.path.parent().ok_or_else(credential_error)?;
        map_credential_result(fs::create_dir_all(parent))
            .and_then(|()| map_credential_result(NamedTempFile::new_in(parent)))
            .and_then(|temporary| write_private_token(temporary, token))
            .and_then(|temporary| map_credential_result(temporary.persist(&self.path)))
            .and_then(|_| make_private(&self.path))
    }

    fn delete(&self) -> Result<(), BlobyardError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(credential_error()),
        }
    }
}

fn write_private_token(
    mut temporary: NamedTempFile,
    token: &SecretString,
) -> Result<NamedTempFile, BlobyardError> {
    make_private(temporary.path())
        .and_then(|()| {
            map_credential_result(
                temporary
                    .write_all(token.expose_secret().as_bytes())
                    .and_then(|()| temporary.flush())
                    .and_then(|()| temporary.as_file().sync_all()),
            )
        })
        .map(|()| temporary)
}

/// Selected token store and any required user warning.
pub struct SelectedTokenStore {
    store: Arc<dyn TokenStore>,
    warning: Option<&'static str>,
}

impl SelectedTokenStore {
    pub(crate) const fn injected(store: Arc<dyn TokenStore>) -> Self {
        Self {
            store,
            warning: None,
        }
    }

    /// Returns the selected store.
    #[must_use]
    pub fn store(&self) -> Arc<dyn TokenStore> {
        Arc::clone(&self.store)
    }

    /// Returns the fallback warning, when applicable.
    #[must_use]
    pub const fn warning(&self) -> Option<&'static str> {
        self.warning
    }
}

impl std::fmt::Debug for SelectedTokenStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectedTokenStore")
            .field("uses_file_fallback", &self.warning.is_some())
            .finish_non_exhaustive()
    }
}

/// Selects the native platform store or an explicit `0600` file fallback.
#[must_use]
pub fn select_token_store(profile: &Slug, fallback_path: impl Into<PathBuf>) -> SelectedTokenStore {
    select_store(
        PlatformTokenStore::for_profile(profile).map(erase_token_store),
        fallback_path.into(),
    )
}

fn erase_token_store<T: TokenStore + 'static>(store: T) -> Arc<dyn TokenStore> {
    Arc::new(store)
}

fn select_store(
    platform: Result<Arc<dyn TokenStore>, BlobyardError>,
    fallback_path: PathBuf,
) -> SelectedTokenStore {
    platform.map_or_else(
        |_| SelectedTokenStore {
            store: Arc::new(FileTokenStore::new(fallback_path)),
            warning: Some(FILE_FALLBACK_WARNING),
        },
        |store| SelectedTokenStore {
            store,
            warning: None,
        },
    )
}

#[cfg(unix)]
fn make_private(path: &Path) -> Result<(), BlobyardError> {
    use std::os::unix::fs::PermissionsExt;
    map_credential_result(fs::set_permissions(path, fs::Permissions::from_mode(0o600)))
}

#[cfg(not(unix))]
fn make_private(_path: &Path) -> Result<(), BlobyardError> {
    Ok(())
}

#[cfg(unix)]
fn ensure_private(path: &Path) -> Result<(), BlobyardError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = map_credential_result(fs::metadata(path))?
        .permissions()
        .mode();
    if mode.trailing_zeros() >= 6 {
        Ok(())
    } else {
        Err(credential_error())
    }
}

#[cfg(not(unix))]
fn ensure_private(_path: &Path) -> Result<(), BlobyardError> {
    Ok(())
}

fn credential_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InternalError,
        "Blobyard couldn't access saved credentials. Check credential-store access and try again.",
    )
}

fn map_credential_result<T, E>(result: Result<T, E>) -> Result<T, BlobyardError> {
    result.map_err(|_| credential_error())
}

#[cfg(test)]
#[path = "token_store_tests.rs"]
mod tests;
