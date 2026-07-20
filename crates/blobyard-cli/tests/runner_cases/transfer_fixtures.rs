#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{SignedReply, ok};

pub(super) fn reservation(
    strategy: &str,
    url: &str,
    part_size: Option<u64>,
    upload_id: &str,
) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "uploadId": upload_id,
            "strategy": strategy,
            "uploadUrl": (strategy == "single").then_some(url),
            "headers": [],
            "partSizeBytes": part_size,
            "expiresAt": "2030-01-01T00:00:00Z"
        }),
        "req_reserve",
    )
}

pub(super) fn part_grants(url: &str, numbers: &[u32]) -> blobyard_api_client::RawResponse {
    let parts = numbers
        .iter()
        .map(|number| serde_json::json!({ "partNumber": number, "uploadUrl": url }))
        .collect::<Vec<_>>();
    ok(
        serde_json::json!({ "parts": parts, "expiresAt": "2030-01-01T00:00:00Z" }),
        "req_parts",
    )
}

pub(super) fn completion(size: u64, checksum: &str) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "uri": "blobyard://team/app/artifact.bin?version=1",
            "sizeBytes": size,
            "checksumSha256": checksum
        }),
        "req_complete",
    )
}

pub(super) fn empty_reply(status: &'static str) -> SignedReply {
    SignedReply {
        status,
        headers: Vec::new(),
        body: Vec::new(),
    }
}

pub(super) fn etag_reply(etag: &'static str) -> SignedReply {
    SignedReply {
        status: "200 OK",
        headers: vec![("ETag", etag)],
        body: Vec::new(),
    }
}
