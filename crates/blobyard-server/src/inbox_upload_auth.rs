use crate::{
    api::AppState,
    auth::{Principal, hash},
    error::ApiError,
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header, request::Parts},
};
use blobyard_contract::{CiAction, InboxRecord};
use blobyard_core::SecretString;
use std::net::SocketAddr;

pub(crate) const INBOX_HEADER: &str = "x-blobyard-inbox-token";
const INVALID_CAPABILITY: &str = "invalid-inbox-capability";

#[derive(Clone, Copy)]
pub(crate) enum RateKind {
    Upload,
    Transfer,
}

impl RateKind {
    const fn limits(self) -> (&'static str, u64, u32) {
        match self {
            Self::Upload => ("upload", 60 * 60 * 1_000, 20),
            Self::Transfer => ("transfer", 60 * 1_000, 120),
        }
    }
}

pub(crate) enum UploadAuthority {
    Operator(Principal),
    Inbox(InboxCredential),
}

pub(crate) enum AuthorizedUpload {
    Operator(Principal),
    Inbox(InboxGuest),
}

pub(crate) struct InboxGuest {
    pub(crate) capability_hash: String,
    pub(crate) fingerprint_hash: String,
    pub(crate) inbox: InboxRecord,
}

pub(crate) struct InboxCredential {
    capability_hash: String,
    fingerprint_hash: String,
    valid: bool,
}

impl UploadAuthority {
    pub(crate) fn authorize_at(
        self,
        state: &AppState,
        kind: RateKind,
        now: u64,
    ) -> Result<AuthorizedUpload, ApiError> {
        match self {
            Self::Operator(principal) => {
                principal.require_action(CiAction::Upload, "object:write")?;
                Ok(AuthorizedUpload::Operator(principal))
            }
            Self::Inbox(credential) => credential.authorize_at(state, kind, now),
        }
    }
}

impl InboxCredential {
    fn authorize_at(
        self,
        state: &AppState,
        kind: RateKind,
        now: u64,
    ) -> Result<AuthorizedUpload, ApiError> {
        let (name, window_ms, limit) = kind.limits();
        let rate_key = hash(&format!(
            "{name}\0{}\0{}",
            self.capability_hash, self.fingerprint_hash
        ));
        crate::inbox_rate::consume(state, &rate_key, window_ms, limit, now)?;
        if !self.valid {
            return Err(ApiError::not_found());
        }
        let inbox = state
            .repository
            .inbox_by_capability(&self.capability_hash, now)
            .map_err(ApiError::concealed_capability)?;
        Ok(AuthorizedUpload::Inbox(InboxGuest {
            capability_hash: self.capability_hash,
            fingerprint_hash: self.fingerprint_hash,
            inbox,
        }))
    }
}

impl FromRequestParts<AppState> for UploadAuthority {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let inbox_header = parts.headers.get(INBOX_HEADER);
        if inbox_header.is_none() {
            return Principal::from_request_parts(parts, state)
                .await
                .map(Self::Operator);
        }
        if parts.headers.contains_key(header::AUTHORIZATION) {
            return Err(ApiError::invalid_token());
        }
        let raw = inbox_header
            .and_then(|value| value.to_str().ok())
            .filter(|value| valid_capability(value));
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or_else(
                || "unavailable".to_owned(),
                |value| value.0.ip().to_string(),
            );
        Ok(Self::Inbox(InboxCredential {
            capability_hash: hash(raw.unwrap_or(INVALID_CAPABILITY)),
            fingerprint_hash: hash(&format!("guest-fingerprint\0{peer}")),
            valid: raw.is_some(),
        }))
    }
}

fn valid_capability(value: &str) -> bool {
    value.strip_prefix("byin_").is_some_and(|suffix| {
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && SecretString::new(value.to_owned()).is_ok()
    })
}

#[cfg(test)]
#[path = "inbox_upload_auth_tests.rs"]
mod tests;
