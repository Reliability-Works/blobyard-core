fn success<T, E>(result: Result<T, E>) -> T {
    assert!(result.is_ok());
    let mut values = result.into_iter().collect::<Vec<_>>();
    assert_eq!(values.len(), 1);
    values.remove(0)
}

fn present<T>(value: Option<T>) -> T {
    let mut values = value.into_iter().collect::<Vec<_>>();
    assert_eq!(values.len(), 1);
    values.remove(0)
}

#[path = "yards_tests/adapter_edges.rs"]
mod adapter_edges;
#[path = "yards_tests/transaction_edges.rs"]
mod transaction_edges;
#[path = "yards_tests/validation_finish.rs"]
mod validation_finish;
#[path = "yards_tests/validation_start.rs"]
mod validation_start;
