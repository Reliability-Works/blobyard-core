//! Executes object-route contracts in the normal library build.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

#[path = "../src/contract_test_support.rs"]
pub mod contract_test_support;

#[path = "../src/objects_contract_tests.rs"]
mod object_contracts;
