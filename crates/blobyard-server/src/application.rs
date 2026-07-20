use crate::{api, auth, error::ServerError, storage_configuration::StorageConfiguration};
#[path = "application_retention.rs"]
mod retention;
use axum::Router;
use blobyard_contract::{CredentialRepository, MetadataRepository, WorkspaceRecord};
use blobyard_core::{GeneratedSecretKind, SecretString, Slug};
use blobyard_repository_sqlite::SqliteRepository;
#[cfg(any(test, feature = "test-seams"))]
use retention::{
    current_time, enforce_plan_with_clock, enforce_project_with_clock, project_workspace,
};
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

#[path = "application_output.rs"]
mod output;
pub use output::show_new_token;

/// A prepared standalone service and optional first-start bootstrap authority.
pub struct InitializedServer {
    router: Router,
    bootstrap_token: Option<SecretString>,
}

impl InitializedServer {
    /// Returns the HTTP application.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Takes bootstrap authority when this call initialized a new installation.
    #[must_use]
    pub const fn take_bootstrap_token(&mut self) -> Option<SecretString> {
        self.bootstrap_token.take()
    }
}

/// Opens durable adapters, creates the default namespace, and installs one-time bootstrap authority
/// only for a never-initialized data directory.
///
/// # Errors
///
/// Returns a stable initialization failure without exposing paths or credentials.
pub fn initialize(data_directory: &Path) -> Result<InitializedServer, ServerError> {
    initialize_with_origins(
        data_directory,
        "http://127.0.0.1:8787",
        "http://localhost:8787",
    )
}

/// Initializes a standalone service with an explicit public transfer origin.
///
/// # Errors
///
/// Returns a stable initialization failure without exposing paths or credentials.
pub fn initialize_with_origin(
    data_directory: &Path,
    public_origin: &str,
) -> Result<InitializedServer, ServerError> {
    initialize_with_origins(data_directory, public_origin, "http://localhost:8787")
}

/// Initializes a standalone service with explicit transfer and Web Yard origins.
///
/// # Errors
///
/// Returns a stable initialization failure without exposing paths or credentials.
pub fn initialize_with_origins(
    data_directory: &Path,
    public_origin: &str,
    web_yard_origin: &str,
) -> Result<InitializedServer, ServerError> {
    initialize_with_storage_origins(
        data_directory,
        public_origin,
        web_yard_origin,
        &StorageConfiguration::Filesystem,
    )
}

fn initialize_with_storage_origins(
    data_directory: &Path,
    public_origin: &str,
    web_yard_origin: &str,
    storage_configuration: &StorageConfiguration,
) -> Result<InitializedServer, ServerError> {
    initialize_with_storage_origins_at(
        data_directory,
        public_origin,
        web_yard_origin,
        storage_configuration,
        retention::current_time(),
    )
}

fn initialize_with_storage_origins_at(
    data_directory: &Path,
    public_origin: &str,
    web_yard_origin: &str,
    storage_configuration: &StorageConfiguration,
    completed_at_ms: Result<u64, ServerError>,
) -> Result<InitializedServer, ServerError> {
    std::fs::create_dir_all(data_directory).map_err(|_error| ServerError::DataDirectory)?;
    let public_origin = crate::normalize_origin(public_origin)?;
    let web_yard_origin = blobyard_core::WebYardOrigin::new(web_yard_origin)
        .map_err(|_error| ServerError::WebYardOrigin)?;
    let repository = Arc::new(SqliteRepository::open(
        &data_directory.join("metadata.sqlite3"),
    )?);
    let storage = storage_configuration.open(data_directory)?;
    crate::yard_cleanup::resume(repository.as_ref(), storage.as_ref(), completed_at_ms?)?;
    let staging_directory = data_directory.join("staging");
    std::fs::create_dir_all(&staging_directory).map_err(|_error| ServerError::DataDirectory)?;
    let capability_key = Arc::new(crate::runtime_secret(data_directory)?);
    let workspace = default_workspace(repository.as_ref())?;
    let token = auth::generate_token(GeneratedSecretKind::BootstrapToken);
    let installed = repository.install_bootstrap(&auth::hash(token.expose_secret()))?;
    Ok(InitializedServer {
        router: api::router(
            repository,
            storage,
            workspace,
            capability_key,
            public_origin,
            web_yard_origin.as_str().to_owned(),
            staging_directory,
        ),
        bootstrap_token: installed.then_some(token),
    })
}

/// Runs the standalone HTTP service until the supplied shutdown future completes.
///
/// # Errors
///
/// Returns initialization, listener, or terminal server I/O failures.
#[doc(hidden)]
pub async fn serve_until(
    listen: SocketAddr,
    data_directory: &Path,
    public_url: Option<&str>,
    web_yard_origin: Option<&str>,
    shutdown: Pin<Box<dyn Future<Output = ()> + Send>>,
) -> Result<(), Box<dyn std::error::Error>> {
    serve_until_with_storage(
        listen,
        data_directory,
        public_url,
        web_yard_origin,
        &StorageConfiguration::Filesystem,
        shutdown,
    )
    .await
}

/// Runs the standalone service with an explicit storage backend until shutdown.
///
/// # Errors
///
/// Returns configuration, initialization, listener, or terminal server I/O failures.
#[doc(hidden)]
pub async fn serve_until_with_storage(
    listen: SocketAddr,
    data_directory: &Path,
    public_url: Option<&str>,
    web_yard_origin: Option<&str>,
    storage_configuration: &StorageConfiguration,
    shutdown: Pin<Box<dyn Future<Output = ()> + Send>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let derived = format!("http://{listen}");
    let derived_yard_origin = format!("http://localhost:{}", listen.port());
    let mut initialized = initialize_with_storage_origins(
        data_directory,
        public_url.unwrap_or(&derived),
        web_yard_origin.unwrap_or(&derived_yard_origin),
        storage_configuration,
    )?;
    show_new_token(initialized.take_bootstrap_token());
    let listener = tokio::net::TcpListener::bind(listen).await?;
    crate::server_result(
        axum::serve(
            listener,
            initialized
                .router()
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await,
    )
}

/// Enforces every enabled standalone retention policy once.
///
/// # Errors
///
/// Returns a stable repository or storage failure. Interrupted plans remain durable for retry.
pub fn enforce_retention(data_directory: &Path) -> Result<(), ServerError> {
    retention::enforce_retention(data_directory)
}

/// Enforces every enabled retention policy using an explicit storage backend.
///
/// # Errors
///
/// Returns a stable repository, configuration, or storage failure.
pub fn enforce_retention_with_storage(
    data_directory: &Path,
    storage_configuration: &StorageConfiguration,
) -> Result<(), ServerError> {
    retention::enforce_retention_with_storage(data_directory, storage_configuration)
}

fn default_workspace(repository: &SqliteRepository) -> Result<WorkspaceRecord, ServerError> {
    default_workspace_with_slug(repository, Slug::new("default".to_owned()))
}

fn default_workspace_with_slug(
    repository: &SqliteRepository,
    slug: Result<Slug, blobyard_core::SlugError>,
) -> Result<WorkspaceRecord, ServerError> {
    let slug = slug.map_err(|_error| ServerError::Initialization)?;
    if let Some(workspace) = repository
        .list_workspaces()?
        .into_iter()
        .find(|workspace| workspace.id == "workspace_default")
    {
        return Ok(workspace);
    }
    let workspace = WorkspaceRecord {
        id: "workspace_default".to_owned(),
        name: "Default".to_owned(),
        slug,
    };
    repository.create_workspace(&workspace)?;
    Ok(workspace)
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "application_seams.rs"]
/// Test-only entry points for deterministic standalone-runtime failures.
pub mod test_seams;

#[cfg(test)]
#[path = "application_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "application_contract_tests.rs"]
mod contract_tests;
