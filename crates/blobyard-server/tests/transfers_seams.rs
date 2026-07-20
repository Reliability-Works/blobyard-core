//! Executes transfer validation contracts in the normal library build.

#![cfg(feature = "test-seams")]

#[path = "../src/contract_test_support.rs"]
pub mod contract_test_support;

#[path = "../src/transfers_contract_tests.rs"]
mod transfer_contracts;
