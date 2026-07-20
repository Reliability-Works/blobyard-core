use blobyard_api_client::Endpoint;
use serde_json::{Value, json};
use std::collections::BTreeSet;

include!("openapi_operations.generated.rs");

pub(super) fn assert_openapi_catalog(listed: &[Value]) {
    let expected_names = OPENAPI_MCP_OPERATIONS
        .iter()
        .map(|(_, _, _, name)| *name)
        .collect::<BTreeSet<_>>();
    let listed_names = listed
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(expected_names.len(), 46);
    assert_eq!(listed_names, expected_names);
    for (operation, expected_path, expected_method, tool_name) in OPENAPI_MCP_OPERATIONS {
        let endpoint = Endpoint::PUBLIC
            .into_iter()
            .find(|endpoint| endpoint.operation_id() == *operation)
            .expect("OpenAPI MCP operation must have a public API endpoint");
        assert_eq!(endpoint.path(), *expected_path, "{operation} path drifted");
        assert_eq!(
            endpoint.method().as_str(),
            *expected_method,
            "{operation} method drifted"
        );
        assert!(
            listed.iter().any(|tool| tool["name"] == *tool_name),
            "{operation} references missing MCP tool {tool_name}"
        );
    }
}

pub(super) fn assert_dashboard_catalog(listed: &[Value]) {
    assert!(listed.iter().all(|tool| !matches!(
        tool["name"].as_str(),
        Some(
            "blobyard_create_billing_checkout"
                | "blobyard_create_billing_portal"
                | "blobyard_create_storage_checkout"
                | "blobyard_create_storage_update"
                | "blobyard_create_billing_subscription_update"
                | "blobyard_prepare_account_deletion"
                | "blobyard_complete_account_deletion"
                | "blobyard_retry_account_deletion"
        )
    )));
}

pub(super) fn assert_yard_catalog(listed: &[Value]) {
    let deploy_yard = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_deploy_web_yard")
        .expect("Web Yard deploy tool must be listed");
    assert_eq!(deploy_yard["annotations"]["openWorldHint"], true);
    assert_eq!(
        deploy_yard["inputSchema"]["required"],
        json!(["directory", "yard", "public"])
    );
    let delete_yard = listed
        .iter()
        .find(|tool| tool["name"] == "blobyard_delete_web_yard")
        .expect("Web Yard delete tool must be listed");
    assert_eq!(delete_yard["annotations"]["destructiveHint"], true);
    assert_eq!(
        delete_yard["inputSchema"]["required"],
        json!(["yard", "confirm"])
    );
}
