//! `SQLite` metadata repository for a single-node Blob Yard runtime.

mod adapter;
mod recovery;

pub use adapter::SqliteRepository;
pub use recovery::{
    DatabaseInspection, current_schema_version, inspect_database, oldest_supported_schema_version,
    snapshot_database,
};
