use super::{Backend, call_tool, call_whoami};
use crate::BackendError;
use serde_json::{Value, json};

#[tokio::test]
async fn backend_failures_are_tool_errors_and_are_sanitized() {
    let backend = Backend::failure(BackendError::new(
        "AUTH_REQUIRED",
        "cookie must be refreshed",
    ));
    let response = call_whoami(&backend, "tool failure must respond").await;
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["data"]["code"],
        "AUTH_REQUIRED"
    );
}

#[tokio::test]
async fn identity_and_list_tools_cannot_leak_public_capabilities() {
    for (name, arguments) in [
        ("blobyard_whoami", json!({})),
        ("blobyard_list_objects", scoped_arguments()),
        ("blobyard_list_shares", json!({ "workspace": "team" })),
        ("blobyard_list_previews", scoped_arguments()),
        ("blobyard_list_inboxes", scoped_arguments()),
    ] {
        let backend = Backend::success(capability_fixture());
        let response = call_tool(&backend, name, arguments, "tool call must respond").await;
        let data = &response["result"]["structuredContent"]["data"][0];
        for key in ["shareUrl", "inboxUrl", "preview_url"] {
            assert_eq!(data[key], "[REDACTED]", "{name} leaked {key}");
        }
    }
}

#[tokio::test]
async fn explicit_issuers_return_only_the_capability_they_created() {
    for (name, arguments, expected) in [
        (
            "blobyard_create_share",
            json!({ "target": "blobyard://team/mobile/artifact.zip" }),
            json!([{
                "shareUrl": "https://blobyard.com/s/share-capability",
                "inboxUrl": "[REDACTED]",
                "preview_url": "[REDACTED]"
            }]),
        ),
        (
            "blobyard_create_preview",
            json!({ "directory": "site" }),
            json!([{
                "shareUrl": "[REDACTED]",
                "inboxUrl": "[REDACTED]",
                "preview_url": "https://preview.blobyard.dev/preview-capability"
            }]),
        ),
        (
            "blobyard_create_inbox",
            json!({ "name": "support" }),
            json!([{
                "shareUrl": "[REDACTED]",
                "inboxUrl": "https://blobyard.com/i/inbox-capability",
                "preview_url": "[REDACTED]"
            }]),
        ),
    ] {
        let backend = Backend::success(capability_fixture());
        let response = call_tool(&backend, name, arguments, "issuer tool must respond").await;
        assert_eq!(
            response["result"]["structuredContent"]["data"], expected,
            "{name} returned the wrong capability policy"
        );
    }
}

#[tokio::test]
async fn issuer_results_still_redact_non_url_capability_material() {
    let backend = Backend::success(json!({
        "shareUrl": "https://blobyard.com/s/issued",
        "shareToken": "raw-token",
        "confirmationCode": "raw-confirmation",
        "capability": "raw-capability",
        "downloadUrl": "https://r2.example/signed"
    }));
    let response = call_tool(
        &backend,
        "blobyard_create_share",
        json!({ "target": "blobyard://team/mobile/artifact.zip" }),
        "share issuer must respond",
    )
    .await;
    let data = &response["result"]["structuredContent"]["data"];
    assert_eq!(data["shareUrl"], "https://blobyard.com/s/issued");
    for key in [
        "shareToken",
        "confirmationCode",
        "capability",
        "downloadUrl",
    ] {
        assert_eq!(data[key], "[REDACTED]");
    }
}

#[tokio::test]
async fn guest_invitation_create_returns_only_its_one_time_url() {
    let invitation_url = format!(
        "https://account.example/account/yard-invite?token=bygi_{}&continuation=opaque",
        "a".repeat(64)
    );
    let output = json!({
        "invitation": { "id": "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
        "invitationUrl": invitation_url,
        "nested": { "invitationUrl": invitation_url },
        "otherUrl": invitation_url,
    });
    let backend = Backend::success(output.clone());
    let created = call_tool(
        &backend,
        "blobyard_create_yard_guest_invite",
        json!({ "yard": "documentation", "email": "guest@example.com" }),
        "guest invitation issuer must respond",
    )
    .await;
    let data = &created["result"]["structuredContent"]["data"];
    assert_eq!(data["invitationUrl"], invitation_url);
    assert_eq!(data["nested"]["invitationUrl"], "[REDACTED]");
    assert_eq!(data["otherUrl"], "[REDACTED]");

    let listed = call_tool(
        &Backend::success(output),
        "blobyard_list_yard_guest_invites",
        json!({ "yard": "documentation" }),
        "guest invitation list must respond",
    )
    .await;
    assert_eq!(
        listed["result"]["structuredContent"]["data"]["invitationUrl"],
        "[REDACTED]"
    );
}

fn scoped_arguments() -> Value {
    json!({ "workspace": "team", "project": "mobile" })
}

fn capability_fixture() -> Value {
    json!([{
        "shareUrl": "https://blobyard.com/s/share-capability",
        "inboxUrl": "https://blobyard.com/i/inbox-capability",
        "preview_url": "https://preview.blobyard.dev/preview-capability"
    }])
}
