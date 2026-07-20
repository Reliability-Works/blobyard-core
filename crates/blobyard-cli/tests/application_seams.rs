//! Executes private application orchestration contracts in the normal library build.

#![cfg(feature = "test-seams")]

#[path = "../src/application_tests.rs"]
mod application_contract;
