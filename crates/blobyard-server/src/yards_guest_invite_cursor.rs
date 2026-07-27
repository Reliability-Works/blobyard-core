use crate::error::ApiError;
use blobyard_contract::YardGuestInviteCursor;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    at: u64,
    id: String,
    yard: String,
}

pub(super) fn encode(yard_id: &str, cursor: &YardGuestInviteCursor) -> String {
    let encoded = serde_json::json!({
        "at": cursor.created_at_ms,
        "id": cursor.id,
        "yard": yard_id,
    });
    super::cursor::encode(&encoded)
}

pub(super) fn decode(
    yard_id: &str,
    value: Option<&str>,
) -> Result<Option<YardGuestInviteCursor>, ApiError> {
    let Some(decoded) = super::cursor::decode::<Cursor>(value)? else {
        return Ok(None);
    };
    let valid = decoded.yard == yard_id
        && decoded.id.starts_with("ygi_")
        && decoded.id.len() == 36
        && decoded.at <= i64::MAX.cast_unsigned();
    if !valid {
        return Err(ApiError::invalid_request());
    }
    Ok(Some(YardGuestInviteCursor {
        created_at_ms: decoded.at,
        id: decoded.id,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::super::cursor::MAXIMUM_CURSOR_LENGTH;
    use super::{Cursor, decode, encode};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use blobyard_contract::YardGuestInviteCursor;

    fn encoded(cursor: &Cursor) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor).expect("cursor"))
    }

    #[test]
    fn cursors_round_trip_and_reject_malformed_or_foreign_positions() {
        let cursor = YardGuestInviteCursor {
            created_at_ms: 42,
            id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        };
        let value = encode("yard_docs", &cursor);
        assert_eq!(
            decode("yard_docs", Some(&value)).expect("decode"),
            Some(cursor)
        );
        assert_eq!(decode("yard_docs", None).expect("absent"), None);

        for malformed in [
            "not-base64".to_owned(),
            URL_SAFE_NO_PAD.encode(b"{}"),
            encoded(&Cursor {
                at: 42,
                id: "wrong".to_owned(),
                yard: "yard_docs".to_owned(),
            }),
            encoded(&Cursor {
                at: 42,
                id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                yard: "yard_other".to_owned(),
            }),
            encoded(&Cursor {
                at: i64::MAX.cast_unsigned() + 1,
                id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                yard: "yard_docs".to_owned(),
            }),
        ] {
            assert!(decode("yard_docs", Some(&malformed)).is_err());
        }
    }

    #[test]
    fn cursor_runtime_limit_accepts_exactly_1024_and_rejects_1025_bytes() {
        let (yard_id, value) = (1..MAXIMUM_CURSOR_LENGTH)
            .find_map(|length| {
                let yard_id = "y".repeat(length);
                let value = encoded(&Cursor {
                    at: 42,
                    id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    yard: yard_id.clone(),
                });
                (value.len() == MAXIMUM_CURSOR_LENGTH).then_some((yard_id, value))
            })
            .expect("exact cursor boundary");
        assert_eq!(
            decode(&yard_id, Some(&value)).expect("exact boundary"),
            Some(YardGuestInviteCursor {
                created_at_ms: 42,
                id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            })
        );
        assert!(decode("yard_docs", Some(&"a".repeat(MAXIMUM_CURSOR_LENGTH + 1))).is_err());
    }
}
