use super::*;

#[tokio::test]
async fn protocol_and_parameter_errors_use_json_rpc_errors() {
    let backend = Backend::success(json!({}));
    let mut server = McpServer::new(&backend);
    assert_eq!(
        server.process_line("{").await.expect("parse error")["error"]["code"],
        -32_700
    );
    assert_eq!(
        server.process_line("[]").await.expect("invalid request")["error"]["code"],
        -32_600
    );
    assert!(
        server
            .process_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await
            .is_none()
    );
    assert_eq!(
        server
            .process_line(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#)
            .await
            .expect("bad version")["error"]["code"],
        -32_600
    );
    assert_eq!(
        server
            .process_line(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#)
            .await
            .expect("bad id")["error"]["code"],
        -32_600
    );
    assert_eq!(
        server
            .process_line(r#"{"jsonrpc":"2.0","id":1}"#)
            .await
            .expect("missing method")["error"]["code"],
        -32_600
    );
    assert_eq!(
        server
            .process_line(&request(1, "initialize", json!({})))
            .await
            .expect("bad initialize")["error"]["code"],
        -32_602
    );
}

#[tokio::test]
async fn initialized_server_rejects_unknown_methods_calls_and_prompts() {
    let backend = Backend::success(json!({}));
    let mut server = initialized_server(&backend).await;
    assert_eq!(
        server
            .process_line(&request(2, "unknown", json!({})))
            .await
            .expect("unknown method")["error"]["code"],
        -32_601
    );
    for params in [
        Value::Null,
        json!({}),
        json!({ "name": "other_whoami" }),
        json!({ "name": "blobyard_whoami", "arguments": { "extra": true } }),
    ] {
        assert_eq!(
            server
                .process_line(&request(3, "tools/call", params))
                .await
                .expect("invalid call")["error"]["code"],
            -32_602
        );
    }
    assert_eq!(
        server
            .process_line(&request(4, "prompts/get", Value::Null))
            .await
            .expect("bad prompt params")["error"]["code"],
        -32_602
    );
    assert_eq!(
        server
            .process_line(&request(5, "prompts/get", json!({ "name": "missing" })))
            .await
            .expect("bad prompt name")["error"]["code"],
        -32_602
    );
    assert_eq!(
        server
            .process_line(&request(5, "prompts/get", json!({})))
            .await
            .expect("missing prompt name")["error"]["code"],
        -32_602
    );
    assert!(
        server
            .process_line(&request(6, "ping", json!({})))
            .await
            .expect("ping")["result"]
            .is_object()
    );
}

#[tokio::test]
async fn resource_errors_are_safe_json_rpc_errors() {
    let backend = Backend::success(json!({}));
    let mut server = initialized_server(&backend).await;
    assert_eq!(
        server
            .process_line(&request(2, "resources/read", Value::Null))
            .await
            .expect("missing resource params")["error"]["code"],
        -32_602
    );
    for uri in [
        "https://example.com/resource",
        "blobyard://projects/mobile",
        "blobyard://projects//objects",
        "blobyard://projects/mobile/missing",
    ] {
        assert_eq!(
            server
                .process_line(&request(3, "resources/read", json!({ "uri": uri })))
                .await
                .expect("unknown resource")["error"]["code"],
            -32_002
        );
    }

    let failing = Backend::failure(BackendError::new("AUTH_REQUIRED", "sign in"));
    let mut failing_server = McpServer::new(&failing);
    initialize(&mut failing_server).await;
    assert_eq!(
        failing_server
            .process_line(&request(
                4,
                "resources/read",
                json!({ "uri": "blobyard://session/identity" })
            ))
            .await
            .expect("backend resource failure")["error"]["code"],
        -32_603
    );
}

#[tokio::test]
async fn stdio_entrypoint_handles_closed_test_input() {
    let backend = Backend::success(json!({}));
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        crate::serve_stdio(&backend),
    )
    .await;
    assert!(result.is_ok(), "test standard input should be closed");
}

#[tokio::test]
async fn serve_emits_only_compact_newline_delimited_json_rpc() {
    let backend = Backend::success(json!({ "message": "first\nsecond" }));
    let input = format!(
        "{}\n{}\n{}\n",
        request(1, "initialize", json!({ "protocolVersion": "2025-11-25" })),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        request(2, "tools/call", json!({ "name": "blobyard_whoami" }))
    );
    let mut output = Vec::new();
    serve(BufReader::new(input.as_bytes()), &mut output, &backend)
        .await
        .expect("in-memory transport must succeed");
    let text = String::from_utf8(output).expect("responses must be UTF-8");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| serde_json::from_str::<Value>(line).is_ok())
    );
    assert!(text.contains("first\\nsecond"));
}
