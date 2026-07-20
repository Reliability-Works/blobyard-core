use super::{RecoveryError, io};
use blobyard_contract::{ObjectChecksum, StorageKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BackupManifest {
    format_version: u32,
    core_version: String,
    pub(super) metadata_schema_version: u32,
    pub(super) metadata_sha256: String,
    pub(super) runtime_secret_sha256: String,
    pub(super) objects: Vec<BackupObject>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BackupObject {
    pub(super) storage_key: String,
    pub(super) size: u64,
    pub(super) checksum: String,
}

impl BackupManifest {
    pub(super) fn new(
        metadata_schema_version: u32,
        metadata_sha256: String,
        runtime_secret_sha256: String,
        mut objects: Vec<BackupObject>,
    ) -> Self {
        objects.sort();
        Self {
            format_version: FORMAT_VERSION,
            core_version: env!("CARGO_PKG_VERSION").to_owned(),
            metadata_schema_version,
            metadata_sha256,
            runtime_secret_sha256,
            objects,
        }
    }

    pub(super) fn read(root: &Path) -> Result<Self, RecoveryError> {
        let bytes = io::read_secure_file(root, Path::new("manifest.json"))?;
        let manifest: Self =
            serde_json::from_slice(&bytes).map_err(|_error| RecoveryError::InvalidBackup)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(super) fn write(&self, root: &Path) -> Result<(), RecoveryError> {
        self.write_with(root, &JsonEncoder)
    }

    fn write_with(&self, root: &Path, encoder: &dyn ManifestEncoder) -> Result<(), RecoveryError> {
        let mut bytes = encoder
            .encode(self)
            .map_err(|()| RecoveryError::Persistence)?;
        bytes.push(b'\n');
        io::write_private_file(&root.join("manifest.json"), &bytes)
    }

    fn validate(&self) -> Result<(), RecoveryError> {
        if self.format_version != FORMAT_VERSION || self.core_version.is_empty() {
            return Err(RecoveryError::InvalidBackup);
        }
        ObjectChecksum::new(self.metadata_sha256.clone())
            .and_then(|_| ObjectChecksum::new(self.runtime_secret_sha256.clone()))
            .map_err(|_error| RecoveryError::InvalidBackup)?;
        let mut keys = BTreeSet::new();
        let mut previous: Option<&BackupObject> = None;
        for object in &self.objects {
            StorageKey::new(object.storage_key.clone())
                .and_then(|_| ObjectChecksum::new(object.checksum.clone()))
                .map_err(|_error| RecoveryError::InvalidBackup)?;
            if !keys.insert(object.storage_key.as_str())
                || previous.is_some_and(|value| value >= object)
            {
                return Err(RecoveryError::InvalidBackup);
            }
            previous = Some(object);
        }
        Ok(())
    }
}

trait ManifestEncoder {
    fn encode(&self, manifest: &BackupManifest) -> Result<Vec<u8>, ()>;
}

struct JsonEncoder;

impl ManifestEncoder for JsonEncoder {
    fn encode(&self, manifest: &BackupManifest) -> Result<Vec<u8>, ()> {
        serde_json::to_vec_pretty(manifest).map_err(|_error| ())
    }
}

impl BackupObject {
    pub(super) const fn new(storage_key: String, size: u64, checksum: String) -> Self {
        Self {
            storage_key,
            size,
            checksum,
        }
    }
}

#[cfg(test)]
#[path = "recovery_manifest_tests.rs"]
mod tests;
