use crate::S3Credentials;
use blobyard_contract::StorageError;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use time::OffsetDateTime;
use url::Url;

pub(crate) fn sign_headers(
    method: &Method,
    url: &Url,
    headers: &mut HeaderMap,
    payload_hash: &str,
    region: &str,
    credentials: &S3Credentials,
    now: OffsetDateTime,
) -> Result<(), StorageError> {
    let timestamp = timestamp(now);
    let date = &timestamp[..8];
    for (name, value) in
        initial_headers(url, payload_hash, &timestamp, credentials.session_token())?
    {
        headers.insert(name, value);
    }
    let (canonical_headers, signed_headers) = canonical_headers(headers)?;
    let request = canonical_request(
        method,
        url,
        &canonical_headers,
        &signed_headers,
        payload_hash,
    );
    let scope = format!("{date}/{region}/s3/aws4_request");
    let request_hash = hex(&Sha256::digest(request.as_bytes()));
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{request_hash}");
    let signature = signature(
        credentials.secret_access_key(),
        date,
        region,
        &string_to_sign,
    );
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id()
    );
    insert_header(headers, "authorization", &authorization)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn canonical_request(
    method: &Method,
    url: &Url,
    headers: &str,
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        url.path(),
        url.query().unwrap_or_default(),
        headers,
        signed_headers,
        payload_hash
    )
}

fn canonical_headers(headers: &HeaderMap) -> Result<(String, String), StorageError> {
    let mut values = BTreeMap::<String, Vec<String>>::new();
    for (name, value) in headers {
        let value = value
            .to_str()
            .map_err(|_error| StorageError::InvalidInput)?;
        values
            .entry(name.as_str().to_ascii_lowercase())
            .or_default()
            .push(normalize_header(value));
    }
    let signed = values.keys().cloned().collect::<Vec<_>>().join(";");
    let mut canonical = String::new();
    for (name, values) in values {
        let _ignored = writeln!(canonical, "{name}:{}", values.join(","));
    }
    Ok((canonical, signed))
}

fn normalize_header(value: &str) -> String {
    value.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

fn canonical_host(url: &Url) -> Result<String, StorageError> {
    let host = url.host_str().ok_or(StorageError::InvalidInput)?;
    Ok(url
        .port()
        .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}")))
}

fn initial_headers(
    url: &Url,
    payload_hash: &str,
    timestamp: &str,
    session_token: Option<&str>,
) -> Result<Vec<(HeaderName, HeaderValue)>, StorageError> {
    let mut values = vec![
        ("host", canonical_host(url)?),
        ("x-amz-content-sha256", payload_hash.to_owned()),
        ("x-amz-date", timestamp.to_owned()),
    ];
    if let Some(token) = session_token {
        values.push(("x-amz-security-token", token.to_owned()));
    }
    values
        .into_iter()
        .map(|(name, value)| {
            HeaderValue::from_str(&value)
                .map(|value| (HeaderName::from_static(name), value))
                .map_err(|_error| StorageError::InvalidInput)
        })
        .collect()
}

fn insert_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), StorageError> {
    let name = HeaderName::from_static(name);
    let value = HeaderValue::from_str(value).map_err(|_error| StorageError::InvalidInput)?;
    headers.insert(name, value);
    Ok(())
}

fn signature(secret: &str, date: &str, region: &str, string_to_sign: &str) -> String {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, b"s3");
    let signing_key = hmac_sha256(&service_key, b"aws4_request");
    hex(&hmac_sha256(&signing_key, string_to_sign.as_bytes()))
}

pub(crate) fn hmac_sha256(key: &[u8], value: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK_SIZE];
    let mut outer_pad = [0x5c_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(value);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn timestamp(value: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second()
    )
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
