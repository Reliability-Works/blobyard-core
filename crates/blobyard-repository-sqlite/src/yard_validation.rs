use super::{rows, transfer_validation};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError,
    YardDeployRecord, YardDeployStatus, is_valid_yard_path,
};
use std::collections::{BTreeMap, HashSet};

pub(super) const HISTORY_DEPTH: usize = 10;
const MAXIMUM_FILES: usize = 10_000;

pub(super) fn start(
    yard: &NewWebYard,
    deploy: &NewYardDeploy,
    event: &NewAuditEvent,
) -> Result<(i64, i64), RepositoryError> {
    for value in [
        &yard.id,
        &yard.workspace_id,
        &yard.project_id,
        &yard.host_label,
        &deploy.id,
        &deploy.yard_id,
        &deploy.workspace_id,
        &deploy.project_id,
        &deploy.client_deploy_id,
        &deploy.manifest_root,
        &deploy.deployment_host_label,
    ] {
        rows::validate_text(value)?;
    }
    let yard_matches = deploy.yard_id == yard.id;
    let workspace_matches = deploy.workspace_id == yard.workspace_id;
    let project_matches = deploy.project_id == yard.project_id;
    let identities_match = yard_matches && workspace_matches && project_matches;
    let valid = identities_match
        && valid_host_label(&yard.host_label)
        && valid_host_label(&deploy.deployment_host_label)
        && valid_client_id(&deploy.client_deploy_id)
        && deploy.manifest_root
            == format!(".blobyard-yard/{}/{}/", yard.id, deploy.client_deploy_id);
    if !valid {
        return Err(RepositoryError::InvalidInput);
    }
    event_matches(
        event,
        "yard.created",
        "web_yard",
        &yard.workspace_id,
        yard.created_at_ms,
        [("yardId", AuditValue::String(yard.id.clone()))],
    )?;
    Ok((
        transfer_validation::to_i64(yard.created_at_ms)?,
        transfer_validation::to_i64(deploy.created_at_ms)?,
    ))
}

pub(super) fn finalise(
    deploy: &YardDeployRecord,
    files: &[NewYardFile],
    now: u64,
) -> Result<(i64, i64, i64), RepositoryError> {
    if !matches!(
        deploy.status,
        YardDeployStatus::Uploading | YardDeployStatus::Finalising
    ) || files.is_empty()
        || files.len() > MAXIMUM_FILES
    {
        return Err(RepositoryError::Conflict);
    }
    let mut paths = HashSet::with_capacity(files.len());
    let mut total = 0_u64;
    let mut count = 0_i64;
    for file in files {
        rows::validate_text(&file.version_id)?;
        if !is_valid_yard_path(&file.normalized_path)
            || !paths.insert(file.normalized_path.as_str())
        {
            return Err(RepositoryError::InvalidInput);
        }
        total = total
            .checked_add(file.byte_size)
            .ok_or(RepositoryError::InvalidInput)?;
        count += 1;
    }
    if !paths.contains("index.html") {
        return Err(RepositoryError::InvalidInput);
    }
    Ok((
        transfer_validation::to_i64(now)?,
        count,
        transfer_validation::to_i64(total)?,
    ))
}

pub(super) fn failure(
    deploy: &YardDeployRecord,
    code: &str,
    message: &str,
    now: u64,
) -> Result<i64, RepositoryError> {
    if !matches!(
        deploy.status,
        YardDeployStatus::Uploading | YardDeployStatus::Finalising
    ) {
        return Err(RepositoryError::Conflict);
    }
    let valid_code = (2..=64).contains(&code.len())
        && code.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    let valid_message =
        (1..=200).contains(&message.len()) && !message.chars().any(char::is_control);
    if !valid_code || !valid_message {
        return Err(RepositoryError::InvalidInput);
    }
    transfer_validation::to_i64(now)
}

pub(super) fn action_event(
    event: &NewAuditEvent,
    action: &str,
    target_type: &str,
    workspace_id: &str,
    at_ms: u64,
    metadata: impl IntoIterator<Item = (&'static str, AuditValue)>,
) -> Result<i64, RepositoryError> {
    event_matches(event, action, target_type, workspace_id, at_ms, metadata)?;
    transfer_validation::to_i64(at_ms)
}

fn event_matches(
    event: &NewAuditEvent,
    action: &str,
    target_type: &str,
    workspace_id: &str,
    at_ms: u64,
    metadata: impl IntoIterator<Item = (&'static str, AuditValue)>,
) -> Result<(), RepositoryError> {
    let expected = metadata
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect::<BTreeMap<_, _>>();
    let actual = event.metadata.iter().cloned().collect::<BTreeMap<_, _>>();
    let valid = event.action == action
        && event.target_type == target_type
        && event.workspace_id == workspace_id
        && event.created_at_ms == at_ms
        && actual.len() == event.metadata.len()
        && actual == expected;
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn valid_client_id(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_host_label(value: &str) -> bool {
    value.contains('-') && blobyard_core::is_valid_dns_label(value)
}
