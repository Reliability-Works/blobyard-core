use crate::commands::DeployArgs;
use crate::config::{YardConfig, validate_yard_name};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SelectedYard {
    pub(super) name: Slug,
    pub(super) directory: PathBuf,
    pub(super) spa: bool,
    pub(super) clean_urls: bool,
}

pub(super) fn select(
    yards: &BTreeMap<String, YardConfig>,
    arguments: &DeployArgs,
) -> Result<Vec<SelectedYard>, BlobyardError> {
    if arguments.all {
        return all(yards, arguments.spa, arguments.clean_urls);
    }
    if let Some(name) = arguments.yard.as_deref() {
        return explicit(yards, arguments, name).map(|yard| vec![yard]);
    }
    automatic(yards, arguments).map(|yard| vec![yard])
}

fn all(
    yards: &BTreeMap<String, YardConfig>,
    spa: bool,
    clean_urls: bool,
) -> Result<Vec<SelectedYard>, BlobyardError> {
    if yards.is_empty() {
        return Err(selection_required());
    }
    yards
        .iter()
        .map(|(name, config)| configured(name, config, None, spa, clean_urls))
        .collect()
}

fn explicit(
    yards: &BTreeMap<String, YardConfig>,
    arguments: &DeployArgs,
    name: &str,
) -> Result<SelectedYard, BlobyardError> {
    let name = validate_yard_name(name)?;
    match (arguments.directory.clone(), yards.get(name.as_str())) {
        (Some(directory), config) => Ok(SelectedYard {
            name,
            directory,
            spa: arguments.spa || config.is_some_and(YardConfig::spa),
            clean_urls: arguments.clean_urls || config.is_some_and(YardConfig::clean_urls),
        }),
        (None, Some(config)) => configured(
            name.as_str(),
            config,
            None,
            arguments.spa,
            arguments.clean_urls,
        ),
        (None, None) => Err(selection_required()),
    }
}

fn automatic(
    yards: &BTreeMap<String, YardConfig>,
    arguments: &DeployArgs,
) -> Result<SelectedYard, BlobyardError> {
    let mut entries = yards.iter();
    match entries.next() {
        None => automatic_unconfigured(arguments, "main"),
        Some((name, config)) if entries.next().is_none() => configured(
            name,
            config,
            arguments.directory.clone(),
            arguments.spa,
            arguments.clean_urls,
        ),
        Some(_) => Err(ambiguous(yards)),
    }
}

fn automatic_unconfigured(
    arguments: &DeployArgs,
    default_name: &str,
) -> Result<SelectedYard, BlobyardError> {
    let directory = arguments.directory.clone().ok_or_else(selection_required)?;
    Ok(SelectedYard {
        name: validate_yard_name(default_name)?,
        directory,
        spa: arguments.spa,
        clean_urls: arguments.clean_urls,
    })
}

fn configured(
    name: &str,
    config: &YardConfig,
    directory: Option<PathBuf>,
    spa: bool,
    clean_urls: bool,
) -> Result<SelectedYard, BlobyardError> {
    Ok(SelectedYard {
        name: validate_yard_name(name)?,
        directory: directory.unwrap_or_else(|| config.directory().to_owned()),
        spa: spa || config.spa(),
        clean_urls: clean_urls || config.clean_urls(),
    })
}

fn selection_required() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "Select a directory to deploy, select a configured Web Yard, or use --all.",
    )
}

fn ambiguous(yards: &BTreeMap<String, YardConfig>) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        format!(
            "Select a Web Yard with --yard or use --all. Configured Web Yards: {}.",
            yards.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{automatic_unconfigured, select};
    use crate::commands::DeployArgs;
    use crate::config::YardConfig;
    use blobyard_core::ErrorCode;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn args() -> DeployArgs {
        DeployArgs {
            directory: None,
            yard: None,
            all: false,
            spa: false,
            clean_urls: false,
            public: true,
        }
    }

    fn yards() -> BTreeMap<String, YardConfig> {
        BTreeMap::from([
            (
                "dashboard".into(),
                YardConfig::from_parts(PathBuf::from("dashboard"), true, false),
            ),
            (
                "marketing".into(),
                YardConfig::from_parts(PathBuf::from("marketing"), false, true),
            ),
        ])
    }

    #[test]
    fn selects_all_configured_yards_in_stable_order() {
        let mut arguments = args();
        arguments.all = true;
        arguments.spa = true;
        arguments.clean_urls = true;
        let selected = select(&yards(), &arguments).expect("all yards");
        assert_eq!(selected[0].name.as_str(), "dashboard");
        assert!(selected[0].spa);
        assert_eq!(selected[1].name.as_str(), "marketing");
        assert!(selected.iter().all(|yard| yard.spa && yard.clean_urls));
        assert_eq!(
            select(&BTreeMap::new(), &arguments)
                .expect_err("empty")
                .code(),
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn explicit_flags_override_or_extend_named_configuration() {
        let mut arguments = args();
        arguments.yard = Some("dashboard".into());
        arguments.directory = Some(PathBuf::from("override"));
        arguments.clean_urls = true;
        let selected = select(&yards(), &arguments).expect("explicit").remove(0);
        assert_eq!(selected.directory, PathBuf::from("override"));
        assert!(selected.spa);
        assert!(selected.clean_urls);

        arguments.directory = None;
        let configured = select(&yards(), &arguments)
            .expect("configured explicit")
            .remove(0);
        assert_eq!(configured.directory, PathBuf::from("dashboard"));

        arguments.yard = Some("new-yard".into());
        arguments.directory = Some(PathBuf::from("override"));
        let selected = select(&yards(), &arguments)
            .expect("new explicit")
            .remove(0);
        assert_eq!(selected.name.as_str(), "new-yard");
        arguments.directory = None;
        assert!(select(&yards(), &arguments).is_err());
        arguments.yard = Some("api".into());
        assert!(select(&yards(), &arguments).is_err());
    }

    #[test]
    fn automatic_selection_requires_exactly_one_configured_yard() {
        let mut arguments = args();
        arguments.directory = Some(PathBuf::from("dist"));
        let selected = select(&BTreeMap::new(), &arguments)
            .expect("main")
            .remove(0);
        assert_eq!(selected.name.as_str(), "main");

        let one = BTreeMap::from([(
            "documentation".into(),
            YardConfig::from_parts(PathBuf::from("configured"), false, true),
        )]);
        let selected = select(&one, &arguments).expect("one").remove(0);
        assert_eq!(selected.directory, PathBuf::from("dist"));
        assert!(selected.clean_urls);
        assert!(
            select(&yards(), &arguments)
                .expect_err("ambiguous")
                .message()
                .contains("dashboard, marketing")
        );
        arguments.directory = None;
        assert!(select(&BTreeMap::new(), &arguments).is_err());

        let invalid = BTreeMap::from([(
            "api".into(),
            YardConfig::from_parts(PathBuf::from("configured"), false, false),
        )]);
        assert!(select(&invalid, &arguments).is_err());
        arguments.directory = Some(PathBuf::from("dist"));
        assert!(automatic_unconfigured(&arguments, "api").is_err());
    }
}
