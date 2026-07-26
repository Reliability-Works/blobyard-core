use blobyard_contract::{NewYardContinuation, YARD_EXCHANGE_CODE_LIFETIME_MS};

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}

fn continuation(return_path: &str) -> NewYardContinuation {
    NewYardContinuation {
        id: "continuation_fixture".to_owned(),
        continuation_hash: hash('a'),
        code_hash: hash('b'),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: return_path.to_owned(),
        created_at_ms: 10,
        expires_at_ms: 10 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}
