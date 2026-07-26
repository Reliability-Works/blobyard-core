#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{environment_kind, environment_lines};
use blobyard_api_client::{YardEnvironmentKind, YardEnvironmentSummary};

fn environment(name: &str, kind: &str) -> YardEnvironmentSummary {
    serde_json::from_value(serde_json::json!({
        "createdAt": "1970-01-01T00:00:00.001Z",
        "id": format!("yardenv_yard_{name}"),
        "kind": kind,
        "name": name,
        "updatedAt": "1970-01-01T00:00:00.002Z"
    }))
    .expect("environment")
}

#[test]
fn environment_lines_cover_empty_and_populated_lists() {
    assert_eq!(environment_lines(&[]), "No environments found.");
    let lines = environment_lines(&[
        environment("production", "production"),
        environment("preview", "preview"),
    ]);
    assert_eq!(
        lines,
        "production\tproduction\tcreated 1970-01-01T00:00:00.001Z\tyardenv_yard_production\n\
         preview\tpreview\tcreated 1970-01-01T00:00:00.001Z\tyardenv_yard_preview"
    );
}

#[test]
fn environment_kinds_present_every_stable_name() {
    assert_eq!(
        environment_kind(YardEnvironmentKind::Production),
        "production"
    );
    assert_eq!(environment_kind(YardEnvironmentKind::Staging), "staging");
    assert_eq!(environment_kind(YardEnvironmentKind::Preview), "preview");
}
