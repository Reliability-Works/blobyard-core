use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_contract::{WorkspaceGroupCursor, WorkspaceGroupMemberCursor};
use serde::Deserialize;

use crate::error::ApiError;

const MAXIMUM_CURSOR_LENGTH: usize = 1_024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    scope: String,
    at_ms: u64,
    key: String,
}

pub(super) fn encode_group(scope: &str, cursor: &WorkspaceGroupCursor) -> String {
    encode(scope, cursor.created_at_ms, &cursor.id)
}

pub(super) fn encode_group_option(
    scope: &str,
    cursor: Option<&WorkspaceGroupCursor>,
) -> Option<String> {
    cursor.map(|value| encode_group(scope, value))
}

pub(super) fn decode_group(
    scope: &str,
    value: Option<&str>,
) -> Result<Option<WorkspaceGroupCursor>, ApiError> {
    decode(scope, value).map(|cursor| {
        cursor.map(|cursor| WorkspaceGroupCursor {
            created_at_ms: cursor.at_ms,
            id: cursor.key,
        })
    })
}

pub(super) fn encode_member(scope: &str, cursor: &WorkspaceGroupMemberCursor) -> String {
    encode(scope, cursor.added_at_ms, &cursor.user_id)
}

pub(super) fn encode_member_option(
    scope: &str,
    cursor: Option<&WorkspaceGroupMemberCursor>,
) -> Option<String> {
    cursor.map(|value| encode_member(scope, value))
}

pub(super) fn decode_member(
    scope: &str,
    value: Option<&str>,
) -> Result<Option<WorkspaceGroupMemberCursor>, ApiError> {
    decode(scope, value).map(|cursor| {
        cursor.map(|cursor| WorkspaceGroupMemberCursor {
            added_at_ms: cursor.at_ms,
            user_id: cursor.key,
        })
    })
}

fn encode(scope: &str, at_ms: u64, key: &str) -> String {
    let value = serde_json::json!({ "scope": scope, "at_ms": at_ms, "key": key });
    URL_SAFE_NO_PAD.encode(value.to_string())
}

fn decode(scope: &str, value: Option<&str>) -> Result<Option<Cursor>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() || value.len() > MAXIMUM_CURSOR_LENGTH {
        return Err(ApiError::invalid_request());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_error| ApiError::invalid_request())?;
    let cursor =
        serde_json::from_slice::<Cursor>(&bytes).map_err(|_error| ApiError::invalid_request())?;
    if cursor.scope == scope {
        Ok(Some(cursor))
    } else {
        Err(ApiError::invalid_request())
    }
}

#[cfg(test)]
#[path = "api_groups_cursor_tests.rs"]
mod tests;
