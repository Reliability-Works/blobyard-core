use crate::{api::AppState, error::ApiError};
use axum::{
    Json,
    extract::{
        FromRequestParts, Query,
        rejection::{JsonRejection, QueryRejection},
    },
    http::request::Parts,
};
use blobyard_contract::{
    CiAction, LocalApiTokenRecord, LocalMachineSessionRecord, ObjectSource, RepositoryError,
};
use blobyard_core::{GeneratedSecretKind, SecretString};
use sha2::{Digest, Sha256};

pub(crate) const OPERATOR_SCOPES: &[&str] = &[
    "audit:read",
    "ci:manage",
    "inbox:manage",
    "object:read",
    "object:write",
    "project:read",
    "project:write",
    "retention:manage",
    "sessions:manage",
    "share:manage",
    "tokens:manage",
    "workspace:read",
    "yard:manage",
    "yard:read",
];

#[derive(Clone, Debug)]
pub(crate) struct Principal(pub(crate) LocalApiTokenRecord);

impl Principal {
    pub(crate) fn require(&self, scope: &str) -> Result<(), ApiError> {
        if self.0.scopes.iter().any(|candidate| candidate == scope) {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }

    pub(crate) fn require_action(
        &self,
        action: CiAction,
        operator_scope: &str,
    ) -> Result<(), ApiError> {
        if self.is_machine() {
            if self.0.scopes.iter().any(|scope| scope == action.as_str()) {
                Ok(())
            } else {
                Err(ApiError::forbidden())
            }
        } else {
            self.require(operator_scope)
        }
    }

    pub(crate) fn require_actions(
        &self,
        actions: &[CiAction],
        operator_scope: &str,
    ) -> Result<(), ApiError> {
        if self.is_machine() {
            if actions
                .iter()
                .all(|action| self.0.scopes.iter().any(|scope| scope == action.as_str()))
            {
                Ok(())
            } else {
                Err(ApiError::forbidden())
            }
        } else {
            self.require(operator_scope)
        }
    }

    pub(crate) fn require_any(&self, scopes: &[&str]) -> Result<(), ApiError> {
        if scopes
            .iter()
            .any(|scope| self.0.scopes.iter().any(|candidate| candidate == scope))
        {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }

    pub(crate) fn apply_json<T, R>(
        &self,
        action: CiAction,
        operator_scope: &str,
        payload: Result<Json<T>, JsonRejection>,
        operation: impl FnOnce(&Self, &T) -> Result<R, ApiError>,
    ) -> Result<R, ApiError> {
        self.require_action(action, operator_scope)?;
        let Json(request) = ApiError::invalid_request_result(payload)?;
        operation(self, &request)
    }

    pub(crate) fn apply_query<T, R>(
        &self,
        action: CiAction,
        operator_scope: &str,
        query: Result<Query<T>, QueryRejection>,
        operation: impl FnOnce(&Self, &T) -> Result<R, ApiError>,
    ) -> Result<R, ApiError> {
        self.require_action(action, operator_scope)?;
        let Query(request) = ApiError::invalid_request_result(query)?;
        operation(self, &request)
    }

    pub(crate) fn object_source(&self) -> ObjectSource {
        if self.is_machine() {
            ObjectSource::Ci
        } else {
            ObjectSource::Cli
        }
    }

    pub(crate) fn is_machine(&self) -> bool {
        self.0.id.starts_with("machine_")
    }
}

macro_rules! managed_json_handler {
    ($name:ident, $request:ty, $response:ty, $action:expr, $scope:literal, $operation:path) => {
        async fn $name(
            axum::extract::State(state): axum::extract::State<crate::api::AppState>,
            principal: crate::auth::Principal,
            payload: Result<axum::Json<$request>, axum::extract::rejection::JsonRejection>,
        ) -> Result<axum::Json<crate::response::Success<$response>>, crate::error::ApiError> {
            principal.apply_json($action, $scope, payload, |principal, request| {
                ($operation)(&state, principal, request, crate::transfer_grants::now_ms())
            })
        }
    };
}

macro_rules! managed_query_handler {
    ($name:ident, $query:ty, $response:ty, $action:expr, $scope:literal, $operation:path) => {
        async fn $name(
            axum::extract::State(state): axum::extract::State<crate::api::AppState>,
            principal: crate::auth::Principal,
            query: Result<axum::extract::Query<$query>, axum::extract::rejection::QueryRejection>,
        ) -> Result<axum::Json<crate::response::Success<$response>>, crate::error::ApiError> {
            principal.apply_query($action, $scope, query, |principal, query| {
                ($operation)(&state, principal, query, crate::transfer_grants::now_ms())
            })
        }
    };
}

pub(crate) use managed_json_handler;
pub(crate) use managed_query_handler;

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|header| header.to_str().ok())
            .ok_or_else(ApiError::auth_required)?;
        let raw = value
            .strip_prefix("Bearer ")
            .ok_or_else(ApiError::auth_required)?;
        let secret = ApiError::invalid_token_result(SecretString::new(raw.to_owned()))?;
        authenticate(state, &secret, crate::transfer_grants::now_ms())
    }
}

fn authenticate(
    state: &AppState,
    secret: &SecretString,
    now_ms: Result<u64, ApiError>,
) -> Result<Principal, ApiError> {
    let now_ms = now_ms?;
    let record = state
        .repository
        .authenticate_api_token(&hash(secret.expose_secret()), now_ms)
        .map_err(credential_error)?;
    if record.id.starts_with("machine_") {
        let session = state
            .repository
            .authenticate_machine_session(&record.id, now_ms)
            .map_err(credential_error)?;
        validate_machine_record(&record, &session)?;
    }
    Ok(Principal(record))
}

fn validate_machine_record(
    token: &LocalApiTokenRecord,
    session: &LocalMachineSessionRecord,
) -> Result<(), ApiError> {
    let scopes = session
        .actions
        .iter()
        .map(|action| action.as_str())
        .collect::<Vec<_>>();
    let valid = token.id == session.id
        && token.workspace_id == session.workspace_id
        && token.project_id.as_deref() == Some(session.project_id.as_str())
        && token.scopes.iter().map(String::as_str).eq(scopes);
    if valid {
        Ok(())
    } else {
        Err(ApiError::invalid_token())
    }
}

const fn credential_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::NotFound | RepositoryError::InvalidInput => ApiError::invalid_token(),
        _ => ApiError::internal(),
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "auth_seams.rs"]
/// Test-only entry points for credential error classification.
pub mod test_seams;

pub(crate) fn generate_token(kind: GeneratedSecretKind) -> SecretString {
    SecretString::from_generated_entropy(kind, generated_entropy())
}

pub(crate) fn generate_preview_host_capability() -> SecretString {
    SecretString::from_preview_host_entropy(generated_entropy())
}

fn generated_entropy() -> [u8; 32] {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut entropy = [0_u8; 32];
    entropy[..16].copy_from_slice(first.as_bytes());
    entropy[16..].copy_from_slice(second.as_bytes());
    entropy
}

pub(crate) fn hash(raw: &str) -> String {
    blobyard_core::hex_digest(&Sha256::digest(raw.as_bytes()))
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
