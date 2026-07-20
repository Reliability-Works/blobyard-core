use super::identities::{ProjectMap, stable_id};
use super::{ExportRetention, ExportShare, HostedMigrationError, ProjectionMaps};
use blobyard_contract::{MigrationRetentionRecord, MigrationShareRecord, ShareStatus};
use blobyard_core::{GeneratedSecretKind, SecretString};
use std::collections::BTreeSet;

pub(super) fn select_shares(
    records: Vec<ExportShare>,
    maps: &ProjectionMaps,
    generate: &mut dyn FnMut(GeneratedSecretKind) -> SecretString,
) -> Result<(Vec<MigrationShareRecord>, Vec<SecretString>), HostedMigrationError> {
    let mut selected = records
        .into_iter()
        .filter(|share| maps.workspaces.contains_key(&share.workspace_reference))
        .filter(|share| maps.versions.contains_key(&share.object_version_reference))
        .collect::<Vec<_>>();
    selected.sort_by_key(|share| (share.created_at, share.share_reference.clone()));
    let mut identifiers = BTreeSet::new();
    let mut destination = Vec::new();
    let mut active_capabilities = Vec::new();
    for share in selected {
        let (status, output_capability) = share_status(&share.status)?;
        let raw = generate(GeneratedSecretKind::ShareCapability);
        let id = stable_id("share", &share.share_reference);
        if !identifiers.insert(id.clone()) {
            return Err(HostedMigrationError::InvalidExport);
        }
        destination.push(MigrationShareRecord {
            id,
            workspace_id: maps.workspaces[&share.workspace_reference].0.clone(),
            version_id: maps.versions[&share.object_version_reference].clone(),
            capability_hash: crate::auth::hash(raw.expose_secret()),
            expires_at_ms: share.expires_at,
            status,
            consumed_count: share.consumed_count,
            maximum_downloads: share.maximum_downloads,
            created_at_ms: share.created_at,
            revoked_at_ms: share.revoked_at,
        });
        if output_capability {
            active_capabilities.push(raw);
        }
    }
    Ok((destination, active_capabilities))
}

fn share_status(value: &str) -> Result<(ShareStatus, bool), HostedMigrationError> {
    match value {
        "active" => Ok((ShareStatus::Active, true)),
        "expired" => Ok((ShareStatus::Active, false)),
        "exhausted" => Ok((ShareStatus::Exhausted, false)),
        "revoked" => Ok((ShareStatus::Revoked, false)),
        _ => Err(HostedMigrationError::InvalidExport),
    }
}

pub(super) fn select_retention(
    records: Vec<ExportRetention>,
    projects: &ProjectMap,
) -> Result<Vec<MigrationRetentionRecord>, HostedMigrationError> {
    let mut selected = records
        .into_iter()
        .filter(|policy| projects.contains_key(&policy.project_reference))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.project_reference.cmp(&right.project_reference));
    let mut identifiers = BTreeSet::new();
    selected
        .into_iter()
        .map(|policy| {
            let project_id = projects[&policy.project_reference].0.clone();
            if !identifiers.insert(project_id.clone()) {
                return Err(HostedMigrationError::InvalidExport);
            }
            Ok(MigrationRetentionRecord {
                project_id,
                keep_latest: policy.keep_latest,
                path_glob: policy.path_glob,
                branch_glob: policy.branch_glob,
                enabled: policy.enabled,
                created_at_ms: policy.created_at,
                updated_at_ms: policy.updated_at,
            })
        })
        .collect()
}
