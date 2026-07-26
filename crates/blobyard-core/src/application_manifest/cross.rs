use super::{ApplicationManifest, DatabaseAccess, FunctionType, ManifestError};
use std::collections::BTreeSet;

pub(super) fn validate(manifest: &ApplicationManifest) -> Vec<ManifestError> {
    let mut errors = Vec::new();
    super::cross_roles::validate(manifest, &mut errors);
    function_references(manifest, &mut errors);
    resource_references(manifest, &mut errors);
    triggers(manifest, &mut errors);
    routes(manifest, &mut errors);
    schedules(manifest, &mut errors);
    errors
}

fn function_references(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    let functions = manifest.functions.as_deref().unwrap_or_default();
    for (index, job) in manifest
        .jobs
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let matching = functions
            .iter()
            .filter(|function| function.name == job.function)
            .collect::<Vec<_>>();
        referenced(
            !matching.is_empty(),
            &format!("jobs[{index}].function"),
            "function",
            &job.function,
            errors,
        );
        if !matching.is_empty()
            && !matching
                .iter()
                .any(|function| function.function_type == FunctionType::Scheduled)
        {
            errors.push(ManifestError::new(
                format!("jobs[{index}].function"),
                "must reference a scheduled function",
            ));
        }
    }
    for (index, route) in manifest
        .routes
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        referenced(
            functions
                .iter()
                .any(|function| function.name == route.function),
            &format!("routes[{index}].function"),
            "function",
            &route.function,
            errors,
        );
    }
    if let Some(health) = &manifest.health {
        referenced(
            functions
                .iter()
                .any(|function| function.name == health.function),
            "health.function",
            "function",
            &health.function,
            errors,
        );
    }
}

fn resource_references(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    let buckets = manifest
        .buckets
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|bucket| bucket.name.as_str())
        .collect::<BTreeSet<_>>();
    for (function_index, function) in manifest
        .functions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        for (grant_index, grant) in function
            .buckets
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let name = grant
                .split_once(':')
                .map_or(grant.as_str(), |(name, _)| name);
            referenced(
                buckets.contains(name),
                &format!("functions[{function_index}].buckets[{grant_index}]"),
                "bucket",
                name,
                errors,
            );
        }
        let uses_database = matches!(
            function.database,
            Some(DatabaseAccess::Read | DatabaseAccess::ReadWrite)
        );
        if uses_database && manifest.database.is_none() {
            errors.push(ManifestError::new(
                format!("functions[{function_index}].database"),
                "database access requires a database section",
            ));
        }
    }
}

fn triggers(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    for (index, function) in manifest
        .functions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        match function.function_type {
            FunctionType::Event => {
                required_trigger(function.event.is_some(), index, "event", errors);
                forbidden_trigger(function.queue.is_some(), index, "queue", errors);
            }
            FunctionType::Queue => {
                required_trigger(function.queue.is_some(), index, "queue", errors);
                forbidden_trigger(function.event.is_some(), index, "event", errors);
            }
            _ => {
                forbidden_trigger(function.event.is_some(), index, "event", errors);
                forbidden_trigger(function.queue.is_some(), index, "queue", errors);
            }
        }
    }
}

fn required_trigger(present: bool, index: usize, field: &str, errors: &mut Vec<ManifestError>) {
    if !present {
        errors.push(ManifestError::new(
            format!("functions[{index}].{field}"),
            format!("is required for a {field} function"),
        ));
    }
}

fn forbidden_trigger(present: bool, index: usize, field: &str, errors: &mut Vec<ManifestError>) {
    if present {
        errors.push(ManifestError::new(
            format!("functions[{index}].{field}"),
            "is not allowed for this function type",
        ));
    }
}

fn routes(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    let mut declared = BTreeSet::new();
    for (index, route) in manifest
        .routes
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        if !declared.insert((route.method, route.path.as_str())) {
            errors.push(ManifestError::new(
                format!("routes[{index}].path"),
                "route path must be unique for its method",
            ));
        }
    }
}

fn schedules(manifest: &ApplicationManifest, errors: &mut Vec<ManifestError>) {
    for (index, job) in manifest
        .jobs
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        if !super::cron::valid(&job.schedule) {
            errors.push(ManifestError::new(
                format!("jobs[{index}].schedule"),
                "must be a valid five-field cron expression",
            ));
        }
        let timezone = jiff::tz::TimeZone::get(&job.timezone);
        let canonical = timezone
            .as_ref()
            .ok()
            .and_then(jiff::tz::TimeZone::iana_name);
        if canonical != Some(job.timezone.as_str()) {
            errors.push(ManifestError::new(
                format!("jobs[{index}].timezone"),
                "must be a canonical IANA timezone identifier",
            ));
        }
    }
}

fn referenced(declared: bool, path: &str, kind: &str, name: &str, errors: &mut Vec<ManifestError>) {
    if !declared {
        errors.push(ManifestError::new(
            path,
            format!("references undeclared {kind} `{name}`"),
        ));
    }
}
