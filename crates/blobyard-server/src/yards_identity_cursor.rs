#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "test fixtures must fail loudly")
)]

use blobyard_contract::{YardManagementRole, YardManagementRoleCursor};
use serde::Deserialize;

use crate::error::ApiError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    scope: String,
    role: String,
    user_id: String,
}

pub(super) fn encode(scope: &str, cursor: &YardManagementRoleCursor) -> String {
    let value = serde_json::json!({
        "role": cursor.role.as_str(),
        "scope": scope,
        "user_id": cursor.user_id,
    });
    super::super::cursor::encode(&value)
}

pub(super) fn decode(
    scope: &str,
    value: Option<&str>,
) -> Result<Option<YardManagementRoleCursor>, ApiError> {
    let Some(cursor) = super::super::cursor::decode::<Cursor>(value)? else {
        return Ok(None);
    };
    if cursor.scope != scope {
        return Err(ApiError::invalid_request());
    }
    let role = YardManagementRole::parse(&cursor.role).ok_or_else(ApiError::invalid_request)?;
    Ok(Some(YardManagementRoleCursor {
        role,
        user_id: cursor.user_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::test_support::error_status;
    use axum::http::StatusCode;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use blobyard_contract::{YardManagementRole, YardManagementRoleCursor};

    #[test]
    fn cursor_round_trips_every_role_and_absence() {
        assert_eq!(decode("yard_fixture", None).expect("absent"), None);
        for role in [
            YardManagementRole::Owner,
            YardManagementRole::Admin,
            YardManagementRole::Developer,
            YardManagementRole::Auditor,
        ] {
            let expected = YardManagementRoleCursor {
                role,
                user_id: "user_fixture".to_owned(),
            };
            let encoded = encode("yard_fixture", &expected);
            assert_eq!(
                decode("yard_fixture", Some(&encoded)).expect("decoded"),
                Some(expected)
            );
        }
    }

    #[test]
    fn cursor_rejects_malformed_oversized_foreign_and_unknown_values() {
        let values = [
            String::new(),
            "a".repeat(1_025),
            "%".to_owned(),
            URL_SAFE_NO_PAD.encode("{}"),
            URL_SAFE_NO_PAD.encode(
                serde_json::json!({
                    "role": "owner",
                    "scope": "yard_other",
                    "user_id": "user_fixture",
                })
                .to_string(),
            ),
            URL_SAFE_NO_PAD.encode(
                serde_json::json!({
                    "role": "reader",
                    "scope": "yard_fixture",
                    "user_id": "user_fixture",
                })
                .to_string(),
            ),
            URL_SAFE_NO_PAD.encode(
                serde_json::json!({
                    "extra": true,
                    "role": "owner",
                    "scope": "yard_fixture",
                    "user_id": "user_fixture",
                })
                .to_string(),
            ),
        ];
        for value in values {
            assert_eq!(
                error_status::<Option<YardManagementRoleCursor>>(decode(
                    "yard_fixture",
                    Some(&value),
                )),
                StatusCode::BAD_REQUEST
            );
        }
    }
}
