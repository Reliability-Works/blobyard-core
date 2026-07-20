//! Duplicated library contract coverage for API validation routes.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

#[path = "../src/contract_test_support.rs"]
pub mod contract_test_support;

#[path = "../src/api_contract_tests.rs"]
mod api_contracts;
