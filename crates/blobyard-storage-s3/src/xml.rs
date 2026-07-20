use blobyard_contract::{MultipartPart, StorageError};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "InitiateMultipartUploadResult")]
struct CreateMultipartResponse {
    #[serde(rename = "UploadId")]
    upload_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "ListBucketResult")]
struct ListObjectsResponse {
    #[serde(rename = "Contents", default)]
    contents: Vec<ListedObject>,
    #[serde(rename = "IsTruncated", default)]
    is_truncated: bool,
    #[serde(rename = "NextContinuationToken")]
    next_continuation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListedObject {
    #[serde(rename = "Key")]
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Error")]
struct ErrorResponse {
    #[serde(rename = "Code")]
    code: Option<String>,
}

pub(crate) struct ListedPage {
    pub(crate) keys: Vec<String>,
    pub(crate) is_truncated: bool,
    pub(crate) next_continuation_token: Option<String>,
}

pub(crate) fn parse_create(bytes: &[u8]) -> Result<String, StorageError> {
    let parsed: CreateMultipartResponse =
        quick_xml::de::from_reader(bytes).map_err(|_error| StorageError::Unavailable)?;
    parsed
        .upload_id
        .filter(|value| !value.is_empty())
        .ok_or(StorageError::Unavailable)
}

pub(crate) fn parse_list(bytes: &[u8]) -> Result<ListedPage, StorageError> {
    let parsed: ListObjectsResponse =
        quick_xml::de::from_reader(bytes).map_err(|_error| StorageError::Unavailable)?;
    let keys = parsed
        .contents
        .into_iter()
        .map(|object| object.key.ok_or(StorageError::Unavailable))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ListedPage {
        keys,
        is_truncated: parsed.is_truncated,
        next_continuation_token: parsed.next_continuation_token,
    })
}

pub(crate) fn parse_error_code(bytes: &[u8]) -> Option<String> {
    quick_xml::de::from_reader::<_, ErrorResponse>(bytes)
        .ok()
        .and_then(|error| error.code)
        .filter(|code| !code.is_empty())
}

pub(crate) fn complete_body(parts: &[MultipartPart]) -> Result<Vec<u8>, StorageError> {
    let mut body = String::from("<CompleteMultipartUpload>");
    for part in parts {
        let tag = part
            .provider_tag
            .as_deref()
            .ok_or(StorageError::InvalidInput)?;
        body.push_str("<Part><PartNumber>");
        body.push_str(&part.number.to_string());
        body.push_str("</PartNumber><ETag>");
        escape_xml(tag, &mut body);
        body.push_str("</ETag></Part>");
    }
    body.push_str("</CompleteMultipartUpload>");
    Ok(body.into_bytes())
}

fn escape_xml(value: &str, target: &mut String) {
    for character in value.chars() {
        match character {
            '&' => target.push_str("&amp;"),
            '<' => target.push_str("&lt;"),
            '>' => target.push_str("&gt;"),
            '"' => target.push_str("&quot;"),
            '\'' => target.push_str("&apos;"),
            other => target.push(other),
        }
    }
}
