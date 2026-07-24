use super::contracts;
use crate::{api::AppState, error::ApiError};
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, Response, StatusCode, header},
};
use blobyard_contract::RepositoryError;

pub(super) async fn public_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    public_fallback_at(
        State(state),
        OriginalUri(uri),
        method,
        headers,
        crate::transfer_grants::now_ms(),
    )
    .await
}

pub(super) async fn public_fallback_at(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let authority = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let host_label =
        authority.and_then(|value| contracts::public_host_label(&state.web_yard_origin, value));
    let Some(host_label) = host_label else {
        return crate::previews::public_fallback(State(state), OriginalUri(uri), method, headers)
            .await;
    };
    if method != Method::GET && method != Method::HEAD {
        return Err(ApiError::not_found());
    }
    let path = contracts::public_request_path(uri.path())?;
    let session = crate::yard_session_cookie::read(&headers);
    let session_hash = session
        .as_ref()
        .map(|token| crate::auth::hash(token.expose_secret()));
    let target =
        state
            .repository
            .yard_file_by_host(&host_label, &path, session_hash.as_deref(), now?);
    let target = match target {
        Ok(target) => target,
        Err(error)
            if redirectable_miss(error) && method == Method::GET && accepts_html(&headers) =>
        {
            let return_path = uri.path_and_query().map_or("/", |value| value.as_str());
            return crate::yard_session_runtime::login_redirect(&state, &host_label, return_path);
        }
        Err(error) => return Err(ApiError::concealed_capability(error)),
    };
    let status = if target.not_found_document {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::OK
    };
    crate::download_io::public_site_response_with_status(
        &state,
        &target.object,
        &headers,
        &method,
        status,
    )
    .await
}

const fn redirectable_miss(error: RepositoryError) -> bool {
    matches!(error, RepositoryError::NotFound)
}

fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|media_range| media_range.split(';').next())
        .any(|media_type| media_type.trim().eq_ignore_ascii_case("text/html"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::redirectable_miss;
    use crate::test_support::error_status;
    use axum::{
        extract::{OriginalUri, State},
        http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    };
    use blobyard_contract::RepositoryError;

    #[test]
    fn only_not_found_is_an_authentication_miss() {
        assert!(redirectable_miss(RepositoryError::NotFound));
        for error in [
            RepositoryError::Conflict,
            RepositoryError::InvalidInput,
            RepositoryError::SchemaTooNew,
            RepositoryError::Unavailable,
        ] {
            assert!(!redirectable_miss(error));
        }
    }

    #[tokio::test]
    async fn yard_public_fallback_propagates_clock_failure() {
        let root = tempfile::tempdir().expect("root");
        let state = crate::test_support::filesystem_state(&root, root.path().join("staging"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("documentation-x.localhost:8787"),
        );
        assert_eq!(
            error_status(
                super::public_fallback_at(
                    State(state),
                    OriginalUri(Uri::from_static("/")),
                    Method::GET,
                    headers,
                    Err(crate::error::ApiError::internal()),
                )
                .await
            ),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
