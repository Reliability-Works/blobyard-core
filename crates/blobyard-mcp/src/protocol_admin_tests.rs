use super::*;
use crate::AdminToolCall;

#[tokio::test]
async fn administration_calls_validate_and_dispatch_exact_arguments() {
    let backend = Backend::success(json!({ "members": [] }));
    let mut server = McpServer::new(&backend);
    initialize(&mut server).await;
    let response = server
        .process_line(&request(
            2,
            "tools/call",
            json!({
                "name": "blobyard_create_invite",
                "arguments": {
                    "workspace": "main",
                    "email": "developer@example.com",
                    "role": "member"
                }
            }),
        ))
        .await
        .expect("administration call must respond");
    assert_eq!(response["result"]["isError"], false);
    {
        let calls = backend.calls.lock().expect("call log lock");
        assert!(matches!(
            calls.as_slice(),
            [ToolCall::Admin(AdminToolCall::CreateInvite { email, role, .. })]
                if email == "developer@example.com" && role == "member"
        ));
        drop(calls);
    }

    let rejected = server
        .process_line(&request(
            3,
            "tools/call",
            json!({
                "name": "blobyard_list_audit",
                "arguments": { "workspace": "main", "secret": "unsafe" }
            }),
        ))
        .await
        .expect("invalid administration call must respond");
    assert!(rejected["error"]["code"].is_number());
}
