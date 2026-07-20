use crate::ToolCall;
use serde_json::Value;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum IssuedCapability {
    None,
    Share,
    Preview,
    Inbox,
}

impl IssuedCapability {
    pub(super) const fn for_call(call: &ToolCall) -> Self {
        match call {
            ToolCall::CreateShare { .. } => Self::Share,
            ToolCall::CreatePreview { .. } => Self::Preview,
            ToolCall::CreateInbox { .. } => Self::Inbox,
            _ => Self::None,
        }
    }
}

pub(super) fn sanitize(value: &mut Value, issued: IssuedCapability) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| sanitize(item, issued)),
        Value::Object(fields) => {
            for (key, item) in fields {
                if sensitive_key(key, issued) {
                    *item = Value::String(REDACTED.to_owned());
                } else {
                    sanitize(item, issued);
                }
            }
        }
        Value::String(text) if sensitive_url(text) => REDACTED.clone_into(text),
        _ => {}
    }
}

fn sensitive_key(key: &str, issued: IssuedCapability) -> bool {
    let key = key.to_ascii_lowercase().replace(['-', '.'], "_");
    if let Some(capability) = public_capability(&key) {
        return capability != issued;
    }
    matches!(
        key.as_str(),
        "downloadurl" | "download_url" | "uploadurl" | "upload_url"
    ) || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("authorization")
        || key.contains("cookie")
        || key.contains("oauth_code")
        || key == "otp"
        || key.contains("confirmation_code")
        || key.contains("confirmationcode")
        || key.contains("signed_url")
        || key.contains("presigned_url")
        || key.contains("capability")
}

fn public_capability(key: &str) -> Option<IssuedCapability> {
    match key {
        "shareurl" | "share_url" => Some(IssuedCapability::Share),
        "previewurl" | "preview_url" => Some(IssuedCapability::Preview),
        "inboxurl" | "inbox_url" => Some(IssuedCapability::Inbox),
        _ => None,
    }
}

fn sensitive_url(text: &str) -> bool {
    let Some((_, query)) = text.split_once('?') else {
        return false;
    };
    let query = query.to_ascii_lowercase();
    [
        "x-amz-signature=",
        "x-amz-credential=",
        "signature=",
        "token=",
        "secret=",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}
