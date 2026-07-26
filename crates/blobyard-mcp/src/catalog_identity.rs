use super::ToolKind;
use crate::catalog_access as access;
use serde_json::{Map, Value};

pub(super) fn contract(
    kind: ToolKind,
    properties: &mut Map<String, Value>,
) -> Option<(&'static str, Vec<&'static str>)> {
    match kind {
        ToolKind::ListYardManagementRoles => {
            Some(access::list_yard_management_roles_contract(properties))
        }
        ToolKind::SetYardManagementRole => {
            Some(access::set_yard_management_role_contract(properties))
        }
        ToolKind::RevokeYardManagementRole => {
            Some(access::revoke_yard_management_role_contract(properties))
        }
        ToolKind::GetYardApplicationPolicy => {
            Some(access::get_yard_application_policy_contract(properties))
        }
        ToolKind::SetYardApplicationPolicy => {
            Some(access::set_yard_application_policy_contract(properties))
        }
        ToolKind::SetYardAccessRoles => Some(access::set_yard_access_roles_contract(properties)),
        _ => None,
    }
}
