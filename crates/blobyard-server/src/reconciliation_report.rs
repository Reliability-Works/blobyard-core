use blobyard_contract::{ObjectChecksum, ObjectVersionRecord, UploadState};
use serde::Serialize;

#[derive(Eq, PartialEq)]
pub(super) struct ObservedObject {
    pub(super) size: u64,
    pub(super) checksum: ObjectChecksum,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReconciliationReport {
    report_schema_version: u32,
    core_version: &'static str,
    metadata_schema_version: u32,
    pub(super) clean: bool,
    counts: ReconciliationCounts,
    pub(super) missing_bytes: Vec<MetadataFinding>,
    pub(super) integrity_disagreements: Vec<IntegrityFinding>,
    pub(super) missing_metadata: Vec<PhysicalFinding>,
    pub(super) orphaned_objects: Vec<MetadataFinding>,
    pub(super) invalid_metadata: Vec<MetadataFinding>,
}

impl ReconciliationReport {
    pub(super) const fn new(schema: u32, metadata_records: usize, physical_objects: usize) -> Self {
        Self {
            report_schema_version: 1,
            core_version: env!("CARGO_PKG_VERSION"),
            metadata_schema_version: schema,
            clean: false,
            counts: ReconciliationCounts::new(metadata_records, physical_objects),
            missing_bytes: Vec::new(),
            integrity_disagreements: Vec::new(),
            missing_metadata: Vec::new(),
            orphaned_objects: Vec::new(),
            invalid_metadata: Vec::new(),
        }
    }

    pub(super) const fn finish(&mut self) {
        self.counts.missing_bytes = self.missing_bytes.len();
        self.counts.integrity_disagreements = self.integrity_disagreements.len();
        self.counts.missing_metadata = self.missing_metadata.len();
        self.counts.orphaned_objects = self.orphaned_objects.len();
        self.counts.invalid_metadata = self.invalid_metadata.len();
        self.counts.findings = self.counts.missing_bytes
            + self.counts.integrity_disagreements
            + self.counts.missing_metadata
            + self.counts.orphaned_objects
            + self.counts.invalid_metadata;
        self.clean = self.counts.findings == 0;
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconciliationCounts {
    metadata_records: usize,
    physical_objects: usize,
    findings: usize,
    missing_bytes: usize,
    integrity_disagreements: usize,
    missing_metadata: usize,
    orphaned_objects: usize,
    invalid_metadata: usize,
}

impl ReconciliationCounts {
    const fn new(metadata_records: usize, physical_objects: usize) -> Self {
        Self {
            metadata_records,
            physical_objects,
            findings: 0,
            missing_bytes: 0,
            integrity_disagreements: 0,
            missing_metadata: 0,
            orphaned_objects: 0,
            invalid_metadata: 0,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MetadataFinding {
    version_id: String,
    storage_key: String,
    state: String,
    reason: &'static str,
}

impl MetadataFinding {
    pub(super) fn from_record(record: &ObjectVersionRecord, reason: &'static str) -> Self {
        Self::invalid(record.id.clone(), record.storage_key.clone(), reason)
            .with_state(record.state)
    }

    const fn invalid(version_id: String, storage_key: String, reason: &'static str) -> Self {
        Self {
            version_id,
            storage_key,
            state: String::new(),
            reason,
        }
    }

    fn with_state(mut self, state: UploadState) -> Self {
        state.as_str().clone_into(&mut self.state);
        self
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PhysicalFinding {
    pub(super) storage_key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IntegrityFinding {
    version_id: String,
    storage_key: String,
    expected_size: u64,
    expected_checksum: String,
    actual_size: Option<u64>,
    actual_checksum: Option<String>,
    reason: &'static str,
}

impl IntegrityFinding {
    pub(super) fn new(
        record: &ObjectVersionRecord,
        expected: &ObservedObject,
        actual: Option<&ObservedObject>,
        reason: &'static str,
    ) -> Self {
        Self {
            version_id: record.id.clone(),
            storage_key: record.storage_key.clone(),
            expected_size: expected.size,
            expected_checksum: expected.checksum.as_str().to_owned(),
            actual_size: actual.map(|value| value.size),
            actual_checksum: actual.map(|value| value.checksum.as_str().to_owned()),
            reason,
        }
    }
}
