use axum::http::StatusCode;

pub(super) fn assert(path: &str, status: StatusCode, failure_index: usize) {
    let expected = if path.starts_with("/transfers/") {
        matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::INTERNAL_SERVER_ERROR
        )
    } else {
        matches!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::UNAUTHORIZED
        )
    };
    assert!(expected, "{path} at failure {failure_index}: {status}");
}
