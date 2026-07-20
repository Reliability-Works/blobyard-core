#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;

fn part(number: u32, dataset: &str) -> ExportPart {
    ExportPart {
        byte_size: 2,
        checksum_sha256: "a".repeat(64),
        dataset: dataset.to_owned(),
        part_number: number,
    }
}

fn index(parts: &[ExportPart], format: &str) -> Vec<u8> {
    let parts = parts
        .iter()
        .map(|part| {
            json!({
                "byteSize": part.byte_size,
                "checksumSha256": part.checksum_sha256,
                "dataset": part.dataset,
                "partNumber": part.part_number,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({
        "dataset": "complete",
        "records": [{ "format": format, "parts": parts }]
    }))
    .expect("index JSON")
}

#[test]
fn index_parser_accepts_the_versioned_bounded_manifest() {
    let parts = vec![part(1, "workspace"), part(2, "versions")];
    assert_eq!(
        parse_index(&index(&parts, "Blob Yard account export v1"), 3)
            .expect("index")
            .len(),
        2
    );
}

#[test]
fn index_parser_rejects_shape_count_and_format_failures() {
    let valid = vec![part(1, "workspace")];
    for (bytes, count) in [
        (b"not-json".to_vec(), 2),
        (
            serde_json::to_vec(&json!({ "dataset": "wrong", "records": [] }))
                .expect("wrong dataset"),
            1,
        ),
        (
            serde_json::to_vec(&json!({ "dataset": "complete", "records": [] }))
                .expect("empty index"),
            1,
        ),
        (
            serde_json::to_vec(&json!({
                "dataset": "complete",
                "records": [
                    { "format": "Blob Yard account export v1", "parts": [] },
                    { "format": "Blob Yard account export v1", "parts": [] }
                ]
            }))
            .expect("duplicate index"),
            1,
        ),
        (index(&valid, "new format"), 2),
        (index(&valid, "Blob Yard account export v1"), 9),
    ] {
        assert_eq!(
            parse_index(&bytes, count).err(),
            Some(HostedMigrationError::InvalidExport)
        );
    }
}

#[test]
fn index_parser_rejects_part_identity_checksum_and_size_failures() {
    let mut duplicate = part(1, "versions");
    let mut zero = part(0, "objects");
    let mut bad_checksum = part(3, "shares");
    bad_checksum.checksum_sha256 = "invalid".to_owned();
    let mut too_large = part(4, "projects");
    too_large.byte_size = MAX_EXPORT_PART_BYTES + 1;
    for parts in [
        vec![part(1, "workspace"), duplicate.clone()],
        vec![zero.clone()],
        vec![bad_checksum.clone()],
        vec![too_large],
    ] {
        assert_eq!(
            validate_parts(&parts),
            Err(HostedMigrationError::InvalidExport)
        );
    }
    assert_eq!(
        parse_index(
            &index(
                std::slice::from_ref(&bad_checksum),
                "Blob Yard account export v1",
            ),
            2,
        )
        .err(),
        Some(HostedMigrationError::InvalidExport)
    );
    duplicate.part_number = 2;
    zero.part_number = 3;
    assert!(validate_parts(&[duplicate, zero]).is_ok());
    assert!(validate_parts(&[part(1, "optional")]).is_ok());
}

#[test]
fn index_parser_rejects_excessive_part_counts_and_total_bytes() {
    let maximum_parts = (0..=MAX_EXPORT_PARTS)
        .map(|index| part(u32::try_from(index + 1).expect("part number"), "optional"))
        .collect::<Vec<_>>();
    assert_eq!(
        parse_index(
            &index(&maximum_parts, "Blob Yard account export v1"),
            maximum_parts.len() + 1,
        )
        .err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let oversized_required = (1..=17)
        .map(|number| {
            let mut required = part(number, "workspace");
            required.byte_size = MAX_EXPORT_PART_BYTES;
            required
        })
        .collect::<Vec<_>>();
    assert_eq!(
        validate_parts(&oversized_required),
        Err(HostedMigrationError::InvalidExport)
    );
}

#[test]
fn signed_urls_allow_https_and_loopback_http_only() {
    for value in [
        "https://objects.example.test/file?signature=secret",
        "http://localhost:3210/file",
        "http://127.0.0.1:3210/file",
        "http://[::1]:3210/file",
    ] {
        let secret = SecretString::new(value).expect("secret URL");
        assert!(signed_url(&secret).is_ok(), "{value}");
    }
    for value in [
        "http://objects.example.test/file",
        "ftp://objects.example.test/file",
        "https://user@objects.example.test/file",
        "https://objects.example.test/file#fragment",
        "not-a-url",
    ] {
        let secret = SecretString::new(value).expect("secret URL");
        assert_eq!(
            signed_url(&secret).err(),
            Some(HostedMigrationError::SourceDownload)
        );
    }
}

#[test]
fn grant_and_checksum_validation_fail_closed() {
    let object = SourceObject {
        version_id: "version".to_owned(),
        uri: "blobyard://workspace/project/file?version=1".to_owned(),
        size: 3,
        checksum: checksum(b"abc"),
    };
    let response = DownloadResponse {
        download_url: SecretString::new("https://objects.example.test/file").expect("URL"),
        filename: "file".to_owned(),
        size_bytes: 3,
        checksum_sha256: object.checksum.clone(),
        expires_at: "2026-07-19T00:00:00Z".to_owned(),
    };
    assert!(validate_grant(&response, &object).is_ok());
    let mut wrong = response;
    wrong.size_bytes = 4;
    assert_eq!(
        validate_grant(&wrong, &object),
        Err(HostedMigrationError::Integrity)
    );
    assert!(valid_checksum(&"f".repeat(64)));
    assert!(!valid_checksum(&"F".repeat(64)));
}
