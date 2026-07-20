#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum RevokeStatus {
    Revoked,
    Invalid,
    AlreadyRevoked,
}

#[derive(serde::Serialize)]
pub(crate) struct RevokeResponse {
    status: RevokeStatus,
}

pub(crate) enum RevokeTarget {
    ApiToken(String),
    CliSession(String),
}

pub(crate) trait CredentialRevokeRequest:
    serde::de::DeserializeOwned + Send + 'static
{
    const REQUIRED_SCOPE: &'static str;

    fn into_target(self) -> RevokeTarget;
}

pub(crate) async fn revoke<T>(
    axum::extract::State(state): axum::extract::State<AppState>,
    principal: crate::auth::Principal,
    payload: Result<axum::Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<axum::Json<crate::response::Success<RevokeResponse>>, crate::error::ApiError>
where
    T: CredentialRevokeRequest,
{
    principal.require(T::REQUIRED_SCOPE)?;
    let axum::Json(request) = crate::error::ApiError::invalid_request_result(payload)?;
    revoke_target(
        &state,
        &principal,
        request.into_target(),
        crate::transfer_grants::now_ms(),
    )
}

pub(crate) fn revoke_target(
    state: &AppState,
    principal: &crate::auth::Principal,
    target: RevokeTarget,
    now: Result<u64, crate::error::ApiError>,
) -> Result<axum::Json<crate::response::Success<RevokeResponse>>, crate::error::ApiError> {
    let now_ms = now?;
    let result = match target {
        RevokeTarget::ApiToken(token_id) => {
            let event = crate::audit::api_token_revoked_event(&principal.0, &token_id, now_ms);
            state.repository.revoke_api_token(&token_id, now_ms, &event)
        }
        RevokeTarget::CliSession(session_id) => {
            let event = crate::audit::cli_session_revoked_event(&principal.0, &session_id, now_ms);
            state.repository.revoke_cli_session(
                &session_id,
                &principal.0.workspace_id,
                now_ms,
                &event,
            )
        }
    };
    revoke_response(result)
}

fn revoke_response(
    result: Result<(), blobyard_contract::RepositoryError>,
) -> Result<axum::Json<crate::response::Success<RevokeResponse>>, crate::error::ApiError> {
    let status = match result {
        Ok(()) => RevokeStatus::Revoked,
        Err(blobyard_contract::RepositoryError::NotFound) => RevokeStatus::Invalid,
        Err(blobyard_contract::RepositoryError::Conflict) => RevokeStatus::AlreadyRevoked,
        Err(error) => return Err(crate::error::ApiError::from_repository(error)),
    };
    Ok(crate::response::success(RevokeResponse { status }))
}
