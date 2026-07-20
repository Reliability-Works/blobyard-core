//! Shared domain primitives for Blobyard clients and services.

mod encoding;
mod error;
mod secret;
mod slug;
mod uri;
mod web_yard_origin;

pub use encoding::hex_digest;
pub use error::{BlobyardError, ErrorCode};
pub use secret::{GeneratedSecretKind, SecretString};
pub use slug::{Slug, SlugError};
pub use uri::{BlobyardUri, BlobyardUriError};
pub use web_yard_origin::{CLOUD_WEB_YARD_ORIGIN, WebYardOrigin, is_valid_dns_label};
