use blobyard_contract::RepositoryError;
use rusqlite::{Connection, Transaction};

pub(super) const CURRENT_SCHEMA_VERSION: u32 = 16;
const MIGRATIONS: [&str; CURRENT_SCHEMA_VERSION as usize] = [
    include_str!("../migrations/0001_initial.sql"),
    include_str!("../migrations/0002_local_auth.sql"),
    include_str!("../migrations/0003_transfers.sql"),
    include_str!("../migrations/0004_downloads.sql"),
    include_str!("../migrations/0005_object_lifecycle.sql"),
    include_str!("../migrations/0006_api_token_lifecycle.sql"),
    include_str!("../migrations/0007_object_provenance.sql"),
    include_str!("../migrations/0008_cli_sessions.sql"),
    include_str!("../migrations/0009_ci_oidc.sql"),
    include_str!("../migrations/0010_multipart_uploads.sql"),
    include_str!("../migrations/0011_shares.sql"),
    include_str!("../migrations/0012_inboxes.sql"),
    include_str!("../migrations/0013_previews.sql"),
    include_str!("../migrations/0014_web_yards.sql"),
    include_str!("../migrations/0015_multipart_provider_tags.sql"),
    include_str!("../migrations/0016_yard_cleanup.sql"),
];

pub(super) fn apply(connection: &mut Connection) -> Result<(), RepositoryError> {
    let version = schema_version(connection)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(RepositoryError::SchemaTooNew);
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .transaction()
        .map_err(|_error| RepositoryError::Unavailable)?;
    apply_pending(&transaction, version)?;
    transaction
        .commit()
        .map_err(|_error| RepositoryError::Unavailable)
}

fn apply_pending(transaction: &Transaction<'_>, version: u32) -> Result<(), RepositoryError> {
    for migration in MIGRATIONS.iter().skip(version as usize) {
        transaction
            .execute_batch(migration)
            .map_err(|_error| RepositoryError::Unavailable)?;
    }
    transaction
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
        .map_err(|_error| RepositoryError::Unavailable)
}

pub(super) fn schema_version(connection: &Connection) -> Result<u32, RepositoryError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_error| RepositoryError::Unavailable)
}

#[cfg(test)]
pub(in crate::adapter) fn apply_through(
    connection: &mut Connection,
    target: u32,
) -> Result<(), RepositoryError> {
    if target > CURRENT_SCHEMA_VERSION {
        return Err(RepositoryError::SchemaTooNew);
    }
    let transaction = connection
        .transaction()
        .map_err(|_error| RepositoryError::Unavailable)?;
    for migration in MIGRATIONS.iter().take(target as usize) {
        transaction
            .execute_batch(migration)
            .map_err(|_error| RepositoryError::Unavailable)?;
    }
    transaction
        .pragma_update(None, "user_version", target)
        .map_err(|_error| RepositoryError::Unavailable)?;
    transaction
        .commit()
        .map_err(|_error| RepositoryError::Unavailable)
}
