use super::support::TestResult;
use crate::S3Credentials;
use crate::signing::{hmac_sha256, sha256_hex, sign_headers};
use crate::xml::{complete_body, parse_create, parse_error_code, parse_list};
use blobyard_contract::{MultipartPart, ObjectChecksum, StorageError};
use blobyard_core::SecretString;
use http::{HeaderMap, HeaderValue, Method};
use std::fmt::Write as _;
use time::{Date, Month, PrimitiveDateTime, Time};

fn credentials(token: Option<&str>) -> Result<S3Credentials, Box<dyn std::error::Error>> {
    Ok(S3Credentials::new(
        SecretString::new("AKIDEXAMPLE")?,
        SecretString::new("example-secret")?,
        token.map(SecretString::new).transpose()?,
    ))
}

fn instant() -> Result<time::OffsetDateTime, time::error::ComponentRange> {
    let date = Date::from_calendar_date(2026, Month::July, 20)?;
    let time = Time::from_hms(1, 2, 3)?;
    Ok(PrimitiveDateTime::new(date, time).assume_utc())
}

#[test]
fn signing_is_deterministic_canonical_and_rejects_invalid_inputs() -> TestResult {
    signing_is_deterministic_and_canonical()?;
    signing_rejects_invalid_inputs()?;
    Ok(())
}

fn signing_is_deterministic_and_canonical() -> TestResult {
    let mut headers = HeaderMap::new();
    headers.append("x-extra", HeaderValue::from_static("  one   two "));
    headers.append("x-extra", HeaderValue::from_static("three"));
    sign_headers(
        &Method::GET,
        &"https://localhost:9443/key?b=2&a=1".parse()?,
        &mut headers,
        &sha256_hex(b"payload"),
        "eu-west-2",
        &credentials(Some("session-token"))?,
        instant()?,
    )?;
    assert_eq!(
        headers.get("host"),
        Some(&HeaderValue::from_static("localhost:9443"))
    );
    assert_eq!(
        headers.get("x-amz-security-token"),
        Some(&HeaderValue::from_static("session-token"))
    );
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or(StorageError::Unavailable)?;
    assert!(authorization.contains("Credential=AKIDEXAMPLE/20260720/eu-west-2/s3/aws4_request"));
    assert!(authorization.contains("SignedHeaders="));
    Ok(())
}

fn signing_rejects_invalid_inputs() -> TestResult {
    let mut invalid_header = HeaderMap::new();
    invalid_header.insert("x-invalid", HeaderValue::from_bytes(&[0xff])?);
    assert_eq!(
        sign_headers(
            &Method::GET,
            &"https://localhost/key".parse()?,
            &mut invalid_header,
            &sha256_hex(b""),
            "eu-west-2",
            &credentials(None)?,
            instant()?,
        ),
        Err(StorageError::InvalidInput)
    );
    let mut no_host = HeaderMap::new();
    assert_eq!(
        sign_headers(
            &Method::GET,
            &"file:///tmp/key".parse()?,
            &mut no_host,
            &sha256_hex(b""),
            "eu-west-2",
            &credentials(None)?,
            instant()?,
        ),
        Err(StorageError::InvalidInput)
    );
    let mut invalid_payload = HeaderMap::new();
    assert_eq!(
        sign_headers(
            &Method::GET,
            &"https://localhost/key".parse()?,
            &mut invalid_payload,
            "bad\npayload",
            "eu-west-2",
            &credentials(None)?,
            instant()?,
        ),
        Err(StorageError::InvalidInput)
    );
    let mut invalid_region = HeaderMap::new();
    assert_eq!(
        sign_headers(
            &Method::GET,
            &"https://localhost/key".parse()?,
            &mut invalid_region,
            &sha256_hex(b""),
            "bad\nregion",
            &credentials(None)?,
            instant()?,
        ),
        Err(StorageError::InvalidInput)
    );
    Ok(())
}

#[test]
fn hmac_sha256_matches_short_and_long_key_vectors() {
    assert_eq!(
        encode(&hmac_sha256(
            b"key",
            b"The quick brown fox jumps over the lazy dog"
        )),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(
        encode(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

fn encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut encoded, byte| {
        let _ = write!(&mut encoded, "{byte:02x}");
        encoded
    })
}

#[test]
fn xml_helpers_cover_success_malformed_missing_and_escaped_values() -> TestResult {
    assert_eq!(parse_create(b"<broken>"), Err(StorageError::Unavailable));
    assert_eq!(
        parse_create(b"<InitiateMultipartUploadResult/>"),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        parse_create(
            b"<InitiateMultipartUploadResult><UploadId></UploadId></InitiateMultipartUploadResult>"
        ),
        Err(StorageError::Unavailable)
    );
    assert_eq!(parse_error_code(b"<broken>"), None);
    assert_eq!(parse_error_code(b"<Error/>"), None);
    assert_eq!(parse_error_code(b"<Error><Code></Code></Error>"), None);
    assert!(matches!(
        parse_list(b"<broken>"),
        Err(StorageError::Unavailable)
    ));

    let part = MultipartPart {
        number: 1,
        size: 1,
        checksum: ObjectChecksum::new(
            "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        )?,
        provider_tag: Some("&<>'\"".to_owned()),
    };
    assert_eq!(
        String::from_utf8(complete_body(&[part])?)?,
        concat!(
            "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>",
            "&amp;&lt;&gt;&apos;&quot;",
            "</ETag></Part></CompleteMultipartUpload>"
        )
    );
    let missing = MultipartPart {
        number: 1,
        size: 1,
        checksum: ObjectChecksum::new(
            "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        )?,
        provider_tag: None,
    };
    assert_eq!(complete_body(&[missing]), Err(StorageError::InvalidInput));
    Ok(())
}
