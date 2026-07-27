#[test]
fn yard_mapper_rejects_non_yard_calls() {
    assert_eq!(
        mcp_yard_command(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .expect_err("wrong call kind")
        .code(),
        ErrorCode::InternalError
    );
    assert_eq!(
        yard_policy_command(WebYardToolCall::ListWebYards {
            scope: Scope::default(),
        })
        .expect_err("wrong policy call")
        .code(),
        ErrorCode::InternalError
    );
    assert_eq!(
        super::identity::command(WebYardToolCall::ListWebYards {
            scope: Scope::default(),
        })
        .expect_err("wrong identity call")
        .code(),
        ErrorCode::InternalError
    );
}
