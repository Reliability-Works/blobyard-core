#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::*;
use keyring_core::{Error, mock};
use std::os::unix::fs::PermissionsExt;

fn mock_store() -> (PlatformTokenStore, keyring_core::Entry) {
    keyring_core::set_default_store(mock::Store::new().expect("mock store"));
    let inner = keyring_core::Entry::new("blobyard-test", "refresh").expect("mock entry");
    let duplicate = keyring_core::Entry::new("blobyard-test", "refresh").expect("mock entry");
    let entry = keyring::Entry { inner };
    (
        PlatformTokenStore::from_result(Ok(entry)).expect("platform wrapper"),
        duplicate,
    )
}

fn set_error(entry: &keyring_core::Entry, error: Error) {
    let credential = entry
        .as_any()
        .downcast_ref::<mock::Cred>()
        .expect("mock credential");
    credential.set_error(error);
}

#[test]
fn platform_store_uses_redacted_mocked_operations() {
    let _ = PlatformTokenStore::new();
    let (store, entry) = mock_store();
    assert!(format!("{store:?}").contains("PlatformTokenStore"));
    assert_eq!(store.load().expect("empty load"), None);

    let token = SecretString::new("refresh-secret").expect("token");
    store.save(&token).expect("save");
    assert_eq!(store.load().expect("load"), Some(token.clone()));
    store.delete().expect("delete");
    assert_eq!(store.load().expect("empty after delete"), None);
    store.delete().expect("repeated delete");

    set_error(&entry, Error::Invalid("test".into(), "test".into()));
    assert!(store.save(&token).is_err());
    set_error(&entry, Error::Invalid("test".into(), "test".into()));
    assert!(store.load().is_err());
    set_error(&entry, Error::Invalid("test".into(), "test".into()));
    assert!(store.delete().is_err());

    entry.set_secret(&[0xff]).expect("invalid utf8 fixture");
    assert!(store.load().is_err());
    assert!(PlatformTokenStore::from_result(Err(Error::NoEntry)).is_err());
}

#[test]
fn profile_accounts_preserve_cloud_and_isolate_self_hosted_credentials() {
    let cloud = Slug::new("cloud").expect("cloud profile");
    let local = Slug::new("local").expect("local profile");
    assert_eq!(profile_account(&cloud), "refresh-token");
    assert_eq!(profile_account(&local), "refresh-token:local");
    assert_ne!(profile_account(&cloud), profile_account(&local));
}

#[test]
fn file_store_is_atomic_private_and_idempotent() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("nested/credentials");
    let store = FileTokenStore::new(&path);
    assert_eq!(store.path(), path);
    assert_eq!(store.load().expect("missing"), None);

    let first = SecretString::new("first-refresh").expect("first");
    let second = SecretString::new("second-refresh").expect("second");
    store.save(&first).expect("first save");
    store.save(&second).expect("atomic replacement");
    assert_eq!(store.load().expect("load"), Some(second));
    assert_eq!(
        fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
        0o600
    );
    store.delete().expect("delete");
    store.delete().expect("idempotent delete");
}

#[test]
fn file_store_rejects_unsafe_or_unreadable_state() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("credentials");
    fs::write(&path, b"secret").expect("fixture");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("permissions");
    assert!(FileTokenStore::new(&path).load().is_err());

    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("permissions");
    fs::write(&path, [0xff]).expect("fixture");
    assert!(FileTokenStore::new(&path).load().is_err());
    assert!(
        FileTokenStore::new(PathBuf::new())
            .save(&SecretString::new("secret").expect("secret"))
            .is_err()
    );

    let directory_target = directory.path().join("target-directory");
    fs::create_dir(&directory_target).expect("directory target");
    assert!(FileTokenStore::new(&directory_target).load().is_err());
    assert!(
        FileTokenStore::new(&directory_target)
            .save(&SecretString::new("secret").expect("secret"))
            .is_err()
    );
    assert!(FileTokenStore::new(&directory_target).delete().is_err());

    let parent_file = directory.path().join("parent-file");
    fs::write(&parent_file, b"not a directory").expect("parent fixture");
    assert!(
        FileTokenStore::new(parent_file.join("credentials"))
            .save(&SecretString::new("secret").expect("secret"))
            .is_err()
    );
    assert!(ensure_private(&directory.path().join("missing")).is_err());
}

#[test]
fn selection_reports_only_explicit_fallback() {
    let directory = tempfile::tempdir().expect("tempdir");
    let platform = erase_token_store(FileTokenStore::new(directory.path().join("platform")));
    let selected = select_store(Ok(platform), directory.path().join("fallback"));
    assert_eq!(selected.warning(), None);
    assert!(format!("{selected:?}").contains("false"));
    assert_eq!(selected.store().load().expect("load"), None);

    let selected = select_store(Err(credential_error()), directory.path().join("fallback"));
    assert_eq!(selected.warning(), Some(FILE_FALLBACK_WARNING));
    assert!(format!("{selected:?}").contains("true"));
    assert_eq!(selected.store().load().expect("load"), None);
}

#[test]
fn credential_result_mapping_is_safe_for_success_and_failure() {
    assert_eq!(map_credential_result::<_, ()>(Ok(7)), Ok(7));
    assert_eq!(
        map_credential_result::<(), _>(Err("provider detail"))
            .expect_err("credential failure")
            .code(),
        ErrorCode::InternalError
    );
}
