//! Stable endpoint inventory contracts.

use blobyard_api_client::{Endpoint, HttpMethod};
use std::collections::BTreeSet;

include!("generated/openapi_operations.rs");

#[test]
fn public_endpoint_inventory_matches_canonical_openapi_operations() {
    let actual = Endpoint::PUBLIC
        .into_iter()
        .map(|endpoint| {
            (
                endpoint.operation_id(),
                endpoint.path(),
                endpoint.method().as_str(),
                endpoint.supports_idempotency(),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected = OPENAPI_API_OPERATIONS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn complete_inventory_is_unique_and_keeps_internal_routes_out_of_public_contract() {
    let identities = Endpoint::ALL
        .into_iter()
        .map(|endpoint| {
            (
                endpoint.operation_id(),
                endpoint.path(),
                endpoint.method().as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(identities.len(), Endpoint::ALL.len());
    assert!(!Endpoint::PUBLIC.contains(&Endpoint::ResolvePreview));
    assert!(!Endpoint::PUBLIC.contains(&Endpoint::StripeWebhook));
    assert_eq!(Endpoint::DeleteObject.method(), HttpMethod::Delete);
    assert_eq!(Endpoint::SetRetention.method(), HttpMethod::Put);
}
