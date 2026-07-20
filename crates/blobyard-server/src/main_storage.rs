use blobyard_core::SecretString;
use blobyard_server::{S3RuntimeConfiguration, StorageConfiguration};
use clap::{Args, ValueEnum};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum StorageBackend {
    #[default]
    Filesystem,
    S3,
}

#[derive(Args, Clone, Debug)]
/// Command-line selection and settings for one durable object-storage backend.
pub struct StorageOptions {
    /// Select filesystem or S3-compatible object storage.
    #[arg(long, value_enum, default_value_t = StorageBackend::Filesystem)]
    storage: StorageBackend,
    /// Root HTTP or HTTPS S3-compatible endpoint.
    #[arg(long)]
    s3_endpoint: Option<String>,
    /// S3 region or provider region identifier.
    #[arg(long, default_value = "us-east-1")]
    s3_region: String,
    /// S3 bucket containing Blob Yard objects.
    #[arg(long)]
    s3_bucket: Option<String>,
    /// Optional relative prefix inside the bucket.
    #[arg(long)]
    s3_prefix: Option<String>,
    /// Use path-style addressing for `MinIO` and compatible providers.
    #[arg(long)]
    s3_force_path_style: bool,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            storage: StorageBackend::Filesystem,
            s3_endpoint: None,
            s3_region: "us-east-1".to_owned(),
            s3_bucket: None,
            s3_prefix: None,
            s3_force_path_style: false,
        }
    }
}

impl StorageOptions {
    pub(super) fn configuration(&self) -> Result<StorageConfiguration, Box<dyn std::error::Error>> {
        self.configuration_with(&environment_variable)
    }

    fn configuration_with(
        &self,
        environment: &dyn Fn(&str) -> Option<String>,
    ) -> Result<StorageConfiguration, Box<dyn std::error::Error>> {
        match self.storage {
            StorageBackend::Filesystem => {
                self.reject_s3_flags()?;
                Ok(StorageConfiguration::Filesystem)
            }
            StorageBackend::S3 => self.s3_configuration(environment),
        }
    }

    fn s3_configuration(
        &self,
        environment: &dyn Fn(&str) -> Option<String>,
    ) -> Result<StorageConfiguration, Box<dyn std::error::Error>> {
        let endpoint = required_option(self.s3_endpoint.as_deref(), "--s3-endpoint")?;
        let bucket = required_option(self.s3_bucket.as_deref(), "--s3-bucket")?;
        let access = required_secret(environment, "BLOBYARD_S3_ACCESS_KEY_ID")?;
        let secret = required_secret(environment, "BLOBYARD_S3_SECRET_ACCESS_KEY")?;
        let session = environment("BLOBYARD_S3_SESSION_TOKEN")
            .map(SecretString::new)
            .transpose()?;
        let config = S3RuntimeConfiguration::new(
            endpoint,
            self.s3_region.clone(),
            bucket,
            access,
            secret,
            session,
        )
        .with_prefix(self.s3_prefix.clone())
        .with_force_path_style(self.s3_force_path_style);
        Ok(StorageConfiguration::S3(config))
    }

    fn reject_s3_flags(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.s3_endpoint.is_some()
            || self.s3_bucket.is_some()
            || self.s3_prefix.is_some()
            || self.s3_force_path_style
        {
            Err("S3 options require --storage s3".into())
        } else {
            Ok(())
        }
    }
}

fn required_option(
    value: Option<&str>,
    name: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    value
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{name} is required for S3 storage").into())
}

fn required_secret(
    environment: &dyn Fn(&str) -> Option<String>,
    name: &'static str,
) -> Result<SecretString, Box<dyn std::error::Error>> {
    let value = environment(name).ok_or_else(|| format!("{name} is required for S3 storage"))?;
    SecretString::new(value).map_err(Into::into)
}

fn environment_variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::{StorageBackend, StorageOptions};

    fn environment(name: &str) -> Option<String> {
        match name {
            "BLOBYARD_S3_ACCESS_KEY_ID" => Some("access".to_owned()),
            "BLOBYARD_S3_SECRET_ACCESS_KEY" => Some("secret".to_owned()),
            "BLOBYARD_S3_SESSION_TOKEN" => Some("session".to_owned()),
            _ => None,
        }
    }

    fn empty_session(name: &str) -> Option<String> {
        match name {
            "BLOBYARD_S3_ACCESS_KEY_ID" => Some("access".to_owned()),
            "BLOBYARD_S3_SECRET_ACCESS_KEY" => Some("secret".to_owned()),
            "BLOBYARD_S3_SESSION_TOKEN" => Some(String::new()),
            _ => None,
        }
    }

    #[test]
    fn storage_configuration_requires_exact_nonsecret_and_secret_inputs() {
        let filesystem = StorageOptions::default();
        assert!(filesystem.configuration_with(&|_name| None).is_ok());
        let mut invalid = filesystem;
        invalid.s3_bucket = Some("bucket".to_owned());
        let error = invalid
            .configuration_with(&|_name| None)
            .err()
            .map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("S3 options require --storage s3"));
        let mut s3 = StorageOptions {
            storage: StorageBackend::S3,
            s3_endpoint: None,
            s3_bucket: None,
            ..StorageOptions::default()
        };
        assert!(s3.configuration_with(&|_name| None).is_err());
        s3.s3_endpoint = Some("http://localhost:9000".to_owned());
        assert!(s3.configuration_with(&|_name| None).is_err());
        s3.s3_bucket = Some("bucket".to_owned());
        assert!(s3.configuration_with(&|_name| None).is_err());
        assert!(
            s3.configuration_with(&|name| {
                (name == "BLOBYARD_S3_ACCESS_KEY_ID").then(|| "access".to_owned())
            })
            .is_err()
        );
        assert_eq!(empty_session("UNKNOWN"), None);
        assert!(s3.configuration_with(&empty_session).is_err());
        assert!(s3.configuration_with(&environment).is_ok());
        assert_eq!(environment("UNKNOWN"), None);
    }
}
