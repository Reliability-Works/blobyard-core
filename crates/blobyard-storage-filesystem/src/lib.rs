//! Durable filesystem object storage for a single-node Blob Yard runtime.

mod filesystem;

pub use filesystem::FilesystemStorage;

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub use filesystem::test_seams;

#[cfg(test)]
extern crate self as blobyard_storage_filesystem;

#[cfg(test)]
#[path = "../tests/conformance.rs"]
mod conformance_contract_tests;

#[cfg(test)]
#[path = "../tests/filesystem_edges.rs"]
mod filesystem_edge_contract_tests;

#[cfg(test)]
#[path = "io_contract_tests.rs"]
mod io_contract_tests;
