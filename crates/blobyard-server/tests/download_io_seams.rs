//! Executes download adapter failure contracts in the normal library build.

#![cfg(feature = "test-seams")]

#[path = "../src/test_support/storage_multipart_macro.rs"]
mod storage_multipart_macro;
#[path = "../src/test_support/storage_put_macro.rs"]
mod storage_put_macro;

#[path = "../src/download_io_contract_tests.rs"]
mod download_io_contracts;
