//! Typed `/v1` protocol and transport behavior for Blobyard clients.

mod client;
mod config;
mod endpoint;
mod endpoint_availability;
mod endpoint_routes;
mod models;
mod protocol;
mod transport;

pub use client::{ApiClient, ApiDeployment, ApiSuccess};
pub use config::{ApiClientConfig, DEFAULT_API_BASE_URL, host_is_loopback};
pub use endpoint::{Endpoint, HttpMethod};
pub use endpoint_availability::OperationAvailability;
pub use models::*;
pub use protocol::{
    ApiCallError, ApiRequest, RawResponse, RetryAdvice, Transport, TransportFuture,
};
pub use transport::ReqwestTransport;
