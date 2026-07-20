#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Standalone HTTP acceptance.

#[path = "http_cases/auth.rs"]
mod auth;
#[cfg(feature = "test-seams")]
#[path = "http_cases/ci.rs"]
mod ci;
#[path = "http_cases/coverage_edges.rs"]
mod coverage_edges;
#[path = "http_cases/coverage_integrity_edges.rs"]
mod coverage_integrity_edges;
#[path = "http_cases/coverage_provider_edges.rs"]
mod coverage_provider_edges;
/// Shared lifecycle HTTP scenarios and fixtures.
#[path = "http_cases/lifecycle.rs"]
pub mod lifecycle;
#[path = "http_cases/lifecycle_delete_http.rs"]
mod lifecycle_delete_http;
#[path = "http_cases/lifecycle_http_edges.rs"]
mod lifecycle_http_edges;
#[path = "http_cases/lifecycle_retention_http.rs"]
mod lifecycle_retention_http;
#[path = "http_cases/lifecycle_storage_retry.rs"]
mod lifecycle_storage_retry;
/// Shared lifecycle database and storage fixtures.
#[path = "http_cases/lifecycle_support.rs"]
pub mod lifecycle_support;
#[path = "http_cases/transfers.rs"]
mod transfers;

/// Shared in-process HTTP fixtures.
pub mod support;
