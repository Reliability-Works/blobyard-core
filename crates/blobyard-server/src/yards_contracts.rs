use crate::{
    error::ApiError,
    site_contracts::{self, SiteManifestError},
};
use blobyard_contract::{NewYardFile, StoredObjectRecord};
use blobyard_core::{Slug, WebYardOrigin};
use sha2::{Digest, Sha256};

const STABLE_HASH_LENGTH: usize = 9;
const DEPLOYMENT_HASH_LENGTH: usize = 10;
const RESERVED_NAMES: &[&str] = &[
    "admin", "api", "app", "assets", "blog", "cdn", "docs", "mail", "status", "support", "www",
];

pub(super) struct YardManifestSnapshot {
    pub(super) files: Vec<NewYardFile>,
    pub(super) total_bytes: u64,
}

pub(super) fn validate_yard_name(name: &Slug) -> Result<(), ApiError> {
    if RESERVED_NAMES.contains(&name.as_str()) {
        Err(ApiError::invalid_request())
    } else {
        Ok(())
    }
}

pub(super) fn stable_host_label(name: &Slug, workspace: &Slug, yard_id: &str) -> String {
    host_label(name, workspace, "yard", yard_id, STABLE_HASH_LENGTH)
}

pub(super) fn deployment_host_label(name: &Slug, workspace: &Slug, deploy_id: &str) -> String {
    host_label(
        name,
        workspace,
        "deployment",
        deploy_id,
        DEPLOYMENT_HASH_LENGTH,
    )
}

fn host_label(name: &Slug, workspace: &Slug, kind: &str, id: &str, hash_length: usize) -> String {
    let hash = opaque_hash(kind, id, hash_length);
    let (yard_part, workspace_part) = balanced_parts(
        name.as_str(),
        workspace.as_str(),
        61_usize.saturating_sub(hash_length),
    );
    format!("{yard_part}-{hash}-{workspace_part}")
}

fn opaque_hash(kind: &str, id: &str, length: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(id.as_bytes());
    blobyard_core::hex_digest(&digest.finalize())[..length].to_owned()
}

fn balanced_parts(left: &str, right: &str, budget: usize) -> (String, String) {
    let mut left_budget = budget.div_ceil(2);
    let mut right_budget = budget - left_budget;
    if left.len() < left_budget {
        right_budget += left_budget - left.len();
    }
    if right.len() < right_budget {
        left_budget += right_budget - right.len();
    }
    (
        bounded_slug(left, left_budget),
        bounded_slug(right, right_budget),
    )
}

fn bounded_slug(value: &str, maximum: usize) -> String {
    value
        .get(..value.len().min(maximum))
        .unwrap_or(value)
        .trim_end_matches('-')
        .to_owned()
}

pub(super) fn web_yard_url(origin: &str, host_label: &str) -> Result<String, ApiError> {
    WebYardOrigin::new(origin)
        .map_err(|_error| ApiError::internal())?
        .url_for(host_label)
        .map_err(|_error| ApiError::internal())
}

pub(super) fn manifest_root(yard_id: &str, client_deploy_id: &str) -> String {
    format!(".blobyard-yard/{yard_id}/{client_deploy_id}/")
}

pub(super) fn snapshot_manifest(
    root: &str,
    objects: Vec<StoredObjectRecord>,
) -> Result<YardManifestSnapshot, ApiError> {
    let objects =
        site_contracts::snapshot_manifest(root, objects, blobyard_contract::is_valid_yard_path)
            .map_err(|error| match error {
                SiteManifestError::Invalid | SiteManifestError::Duplicate => {
                    ApiError::invalid_request()
                }
            })?;
    let mut files = Vec::with_capacity(objects.len());
    let mut total_bytes = 0_u64;
    for object in objects {
        let byte_size = object.byte_size.ok_or_else(ApiError::invalid_request)?;
        total_bytes = total_bytes
            .checked_add(byte_size)
            .ok_or_else(ApiError::invalid_request)?;
        files.push(NewYardFile {
            normalized_path: object.normalized_path,
            version_id: object.version_id,
            byte_size,
        });
    }
    Ok(YardManifestSnapshot { files, total_bytes })
}

pub(super) fn public_host_label(origin: &str, authority: &str) -> Option<String> {
    let origin = WebYardOrigin::new(origin).ok()?;
    let suffix = format!(".{}", origin.authority());
    let label = authority.strip_suffix(&suffix)?;
    valid_host_label(label).then(|| label.to_owned())
}

fn valid_host_label(value: &str) -> bool {
    value.contains('-') && blobyard_core::is_valid_dns_label(value)
}

pub(super) fn public_request_path(path: &str) -> Result<String, ApiError> {
    site_contracts::public_path(path, "", "/", blobyard_contract::is_valid_yard_request_path)
        .map_err(|_error| ApiError::not_found())
}
