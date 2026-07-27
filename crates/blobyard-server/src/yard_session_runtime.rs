use crate::{api::AppState, error::ApiError, yard_session_contracts, yard_session_cookie};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, Response, StatusCode, header},
    routing::any,
};
use blobyard_contract::{
    NewYardSession, RepositoryError, YARD_SESSION_LIFETIME_MS, YardIdentity,
    YardSessionAuditContext,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/.blobyard/session/exchange", any(exchange_dispatch))
        .route("/.blobyard/session/identity", any(identity_dispatch))
        .route("/.blobyard/session/logout", any(logout_dispatch))
}

async fn identity_dispatch(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    let host_label = get_host(&state, &request)?;
    require_same_origin(&state, &host_label, request.headers())?;
    let token = yard_session_cookie::read(request.headers()).ok_or_else(ApiError::not_found)?;
    resolve_identity_at(
        &state,
        &host_label,
        &token,
        crate::transfer_grants::now_ms(),
    )
}

fn resolve_identity_at(
    state: &AppState,
    host_label: &str,
    token: &SecretString,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let identity = state
        .repository
        .resolve_yard_identity(host_label, &crate::auth::hash(token.expose_secret()), now?)
        .map_err(identity_error)?;
    identity_response(&identity)
}

async fn exchange_dispatch(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    let host_label = get_host(&state, &request)?;
    let code = request.uri().query().and_then(single_code).and_then(|raw| {
        yard_session_contracts::has_token_shape(&raw, "byx_")
            .then(|| SecretString::new(raw).ok())
            .flatten()
    });
    let Some(code) = code else {
        return fresh_login_redirect(&state, &host_label);
    };
    exchange(&state, &host_label, &code)
}

fn get_host(state: &AppState, request: &Request<Body>) -> Result<String, ApiError> {
    if request.method() != Method::GET {
        return Err(ApiError::not_found());
    }
    yard_host(state, request.headers())
}

fn exchange(
    state: &AppState,
    host_label: &str,
    code: &SecretString,
) -> Result<Response<Body>, ApiError> {
    exchange_at(state, host_label, code, crate::transfer_grants::now_ms())
}

fn exchange_at(
    state: &AppState,
    host_label: &str,
    code: &SecretString,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let now = now?;
    let token = crate::auth::generate_token(GeneratedSecretKind::YardSession);
    let session = NewYardSession {
        id: format!("yardsession_{}", uuid::Uuid::new_v4().simple()),
        token_hash: crate::auth::hash(token.expose_secret()),
        created_at_ms: now,
        expires_at_ms: now
            .checked_add(YARD_SESSION_LIFETIME_MS)
            .ok_or_else(ApiError::internal)?,
    };
    let audit = YardSessionAuditContext {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        request_id: crate::error::request_id(),
    };
    let exchanged = state.repository.exchange_yard_session_code(
        &crate::auth::hash(code.expose_secret()),
        host_label,
        &session,
        &audit,
        now,
    );
    let exchanged = match exchanged {
        Ok(exchanged) => exchanged,
        Err(error) => {
            exchange_failure(error)?;
            return fresh_login_redirect(state, host_label);
        }
    };
    exchanged_redirect(&exchanged.return_path, session_cookie(&token))
}

fn exchanged_redirect(
    return_path: &str,
    cookie: Result<axum::http::HeaderValue, ApiError>,
) -> Result<Response<Body>, ApiError> {
    crate::response::redirect(StatusCode::SEE_OTHER, return_path, Some(cookie?))
}

async fn logout_dispatch(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    if request.method() != Method::POST {
        return Err(ApiError::not_found());
    }
    let host_label = yard_host(&state, request.headers())?;
    require_same_origin(&state, &host_label, request.headers())?;
    if let Some(token) = yard_session_cookie::read(request.headers()) {
        revoke_cookie(
            &state,
            &host_label,
            &token,
            crate::transfer_grants::now_ms(),
        )?;
    }
    crate::response::redirect(
        StatusCode::SEE_OTHER,
        "/",
        Some(yard_session_cookie::clear_header()),
    )
}

pub(crate) fn fresh_login_redirect(
    state: &AppState,
    host_label: &str,
) -> Result<Response<Body>, ApiError> {
    fresh_login_redirect_at(state, host_label, crate::transfer_grants::now_ms())
}

fn fresh_login_redirect_at(
    state: &AppState,
    host_label: &str,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let continuation =
        yard_session_contracts::issue(&state.yard_continuation_key, host_label, "/", now?)?;
    let location = yard_session_contracts::login_url(&state.public_origin, &continuation)?;
    crate::response::redirect(StatusCode::FOUND, &location, None)
}

pub(crate) fn login_redirect(
    state: &AppState,
    host_label: &str,
    return_path: &str,
) -> Result<Response<Body>, ApiError> {
    login_redirect_at(
        state,
        host_label,
        return_path,
        crate::transfer_grants::now_ms(),
    )
}

fn login_redirect_at(
    state: &AppState,
    host_label: &str,
    return_path: &str,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let continuation =
        yard_session_contracts::issue(&state.yard_continuation_key, host_label, return_path, now?)?;
    let location = yard_session_contracts::login_url(&state.public_origin, &continuation)?;
    crate::response::redirect(StatusCode::FOUND, &location, None)
}

fn yard_host(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|authority| {
            yard_session_contracts::yard_host_label(&state.web_yard_origin, authority)
        })
        .ok_or_else(ApiError::not_found)
}

fn require_same_origin(
    state: &AppState,
    host_label: &str,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let supplied = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if let Some(supplied) = supplied {
        let expected = expected_origin(yard_session_contracts::yard_url(
            &state.web_yard_origin,
            host_label,
        ))?;
        if supplied != expected {
            return Err(ApiError::not_found());
        }
    }
    Ok(())
}

fn expected_origin(yard_url: Result<String, ApiError>) -> Result<String, ApiError> {
    Ok(parsed_origin(&yard_url?)?.origin().ascii_serialization())
}

fn parsed_origin(value: &str) -> Result<url::Url, ApiError> {
    url::Url::parse(value).map_err(|_error| ApiError::internal())
}

fn session_cookie(token: &SecretString) -> Result<axum::http::HeaderValue, ApiError> {
    session_cookie_result(yard_session_cookie::set_header(token))
}

fn session_cookie_result(
    result: Result<axum::http::HeaderValue, ()>,
) -> Result<axum::http::HeaderValue, ApiError> {
    result.map_err(|()| ApiError::internal())
}

fn revoke_cookie(
    state: &AppState,
    host_label: &str,
    token: &SecretString,
    now: Result<u64, ApiError>,
) -> Result<(), ApiError> {
    logout_result(state.repository.revoke_yard_session_by_token(
        &crate::auth::hash(token.expose_secret()),
        host_label,
        now?,
    ))
}

fn single_code(query: &str) -> Option<String> {
    let mut values = url::form_urlencoded::parse(query.as_bytes());
    let value = values
        .next()
        .filter(|(name, _value)| name == "code")?
        .1
        .into_owned();
    values.next().is_none().then_some(value)
}

const fn exchange_failure(error: RepositoryError) -> Result<(), ApiError> {
    match error {
        RepositoryError::NotFound => Ok(()),
        RepositoryError::Conflict
        | RepositoryError::InvalidInput
        | RepositoryError::SchemaTooNew
        | RepositoryError::Unavailable => Err(ApiError::internal()),
    }
}

fn logout_result(result: Result<bool, RepositoryError>) -> Result<(), ApiError> {
    result
        .map(|_revoked| ())
        .map_err(|_error| ApiError::internal())
}

fn identity_response(identity: &YardIdentity) -> Result<Response<Body>, ApiError> {
    let management_role = identity
        .management_role
        .map_or(serde_json::Value::Null, |role| {
            serde_json::Value::String(role.as_str().to_owned())
        });
    let body = serde_json::json!({
        "userId": identity.user_id,
        "workspaceId": identity.workspace_id,
        "projectId": identity.project_id,
        "yardId": identity.yard_id,
        "environmentId": identity.environment_id,
        "displayName": identity.display_name,
        "email": identity.email,
        "groups": identity.groups,
        "managementRole": management_role,
        "appRoles": identity.app_roles,
        "permissions": identity.permissions,
        "sessionId": identity.session_id,
    });
    ApiError::internal_result(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CACHE_CONTROL, "private, no-store")
            .body(Body::from(body.to_string())),
    )
}

const fn identity_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::NotFound => ApiError::not_found(),
        RepositoryError::Conflict
        | RepositoryError::InvalidInput
        | RepositoryError::SchemaTooNew
        | RepositoryError::Unavailable => ApiError::internal(),
    }
}

#[cfg(test)]
#[path = "yard_session_runtime_tests.rs"]
mod tests;
