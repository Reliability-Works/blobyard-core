//! Executes deterministic filesystem failure contracts in the normal library build.

#![cfg(feature = "test-seams")]

#[path = "../src/io_contract_tests.rs"]
mod io_contracts;
