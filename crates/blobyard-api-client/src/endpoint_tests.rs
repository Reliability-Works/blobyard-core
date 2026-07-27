use super::{Endpoint, public_endpoints};
use crate::OperationAvailability;
use serde_json::Value;
use std::collections::BTreeSet;

const CLOUD_YARD_ENDPOINTS: [Endpoint; 13] = [
    Endpoint::ListYardEnvironments,
    Endpoint::GetYardAccess,
    Endpoint::SetYardVisibility,
    Endpoint::GrantYardAccess,
    Endpoint::RevokeYardAccess,
    Endpoint::ListYardManagementRoles,
    Endpoint::SetYardManagementRole,
    Endpoint::RevokeYardManagementRole,
    Endpoint::GetYardApplicationPolicy,
    Endpoint::SetYardApplicationPolicy,
    Endpoint::SetYardAccessRoles,
    Endpoint::ListYardSessions,
    Endpoint::RevokeYardSession,
];

const LOCAL_USER_ENDPOINTS: [Endpoint; 4] = [
    Endpoint::CreateLocalUser,
    Endpoint::ListLocalUsers,
    Endpoint::ResetLocalUserLoginKey,
    Endpoint::DeactivateLocalUser,
];

#[test]
fn runtime_public_inventory_excludes_only_internal_routes() {
    let public = std::hint::black_box(public_endpoints());
    assert_eq!(public, Endpoint::PUBLIC);
    assert_eq!(public.len() + 2, Endpoint::ALL.len());
    assert!(!public.contains(&Endpoint::ResolvePreview));
    assert!(!public.contains(&Endpoint::StripeWebhook));
}

#[test]
fn runtime_ownership_matches_the_canonical_operation_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    let ownership: Value =
        serde_json::from_str(include_str!("../../../openapi/operation-ownership.json"))?;
    let expected_core = identifiers(&ownership, "core")?;
    let expected_hosted = identifiers(&ownership, "hostedExtension")?;
    let mut actual_core = identifiers_for(OperationAvailability::Core);
    actual_core.extend(identifiers_for(OperationAvailability::SelfHostedOnly));
    let actual_hosted = identifiers_for(OperationAvailability::HostedExtension);
    assert_eq!(actual_core, expected_core);
    assert_eq!(actual_hosted, expected_hosted);
    assert_eq!(
        Endpoint::ExchangeBootstrapToken.availability(),
        OperationAvailability::SelfHostedOnly
    );
    for endpoint in LOCAL_USER_ENDPOINTS {
        assert_eq!(
            endpoint.availability(),
            OperationAvailability::SelfHostedOnly
        );
    }
    for endpoint in CLOUD_YARD_ENDPOINTS {
        assert_eq!(endpoint.availability(), OperationAvailability::Core);
    }
    assert_eq!(
        Endpoint::ResolvePreview.availability(),
        OperationAvailability::Internal
    );
    assert_eq!(
        Endpoint::StripeWebhook.availability(),
        OperationAvailability::Internal
    );
    Ok(())
}

fn identifiers(
    document: &Value,
    key: &str,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    document
        .get(key)
        .ok_or("missing ownership list")?
        .as_array()
        .ok_or("ownership list is not an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| "operation id is not a string".into())
        })
        .collect()
}

fn identifiers_for(availability: OperationAvailability) -> BTreeSet<String> {
    Endpoint::PUBLIC
        .iter()
        .filter(|endpoint| endpoint.availability() == availability)
        .map(|endpoint| endpoint.operation_id().to_owned())
        .collect()
}
