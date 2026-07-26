#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

pub(super) fn item<'a>(value: &'a serde_json::Value, token_id: &str) -> &'a serde_json::Value {
    value["data"]
        .as_array()
        .expect("token list")
        .iter()
        .find(|item| item["id"] == token_id)
        .expect("token summary")
}
