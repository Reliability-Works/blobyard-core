//! Executes retained-plan contracts in the normal library build.

#![cfg(feature = "test-seams")]

#[path = "../src/application_contract_tests.rs"]
mod application_contracts;
