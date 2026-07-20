use crate::error::{ApiError, request_id};
use axum::{
    Json,
    body::Body,
    http::{Response, StatusCode, header},
};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Success<T> {
    ok: bool,
    data: T,
    request_id: String,
}

pub(crate) fn success<T: Serialize>(data: T) -> Json<Success<T>> {
    success_with_request(data, request_id())
}

pub(crate) const fn success_with_request<T: Serialize>(
    data: T,
    request_id: String,
) -> Json<Success<T>> {
    Json(Success {
        ok: true,
        data,
        request_id,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
}

pub(crate) const fn page<T>(items: Vec<T>) -> Page<T> {
    Page {
        items,
        next_cursor: None,
    }
}

pub(crate) fn secure_html(html: String, policy: &'static str) -> Result<Response<Body>, ApiError> {
    ApiError::internal_result(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::REFERRER_POLICY, "no-referrer")
            .header("content-security-policy", policy)
            .header("x-content-type-options", "nosniff")
            .body(Body::from(html)),
    )
}

#[derive(Serialize)]
pub(crate) struct Health {
    status: &'static str,
    version: &'static str,
}

pub(crate) async fn health() -> Json<Success<Health>> {
    success(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
