//! Validation, serde, and redaction behavior for core value types.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_core::{BlobyardUri, ErrorCode, GeneratedSecretKind, SecretString, Slug};
use std::str::FromStr;

#[test]
fn secrets_validate_redact_clone_compare_and_round_trip() {
    let secret = SecretString::new("opaque-token").expect("valid secret");
    let clone = secret.clone();
    let different = SecretString::new("other-token").expect("valid secret");

    assert_eq!(secret.expose_secret(), "opaque-token");
    assert_eq!(secret, clone);
    assert_ne!(secret, different);
    assert_eq!(format!("{secret:?}"), "[REDACTED]");
    assert_eq!(
        serde_json::to_string(&secret).expect("serialize secret"),
        "\"opaque-token\""
    );
    assert_eq!(
        serde_json::from_str::<SecretString>("\"opaque-token\"").expect("deserialize secret"),
        secret
    );
}

#[test]
fn secrets_reject_empty_control_and_oversized_values() {
    let cases = [String::new(), "line\nbreak".into(), "x".repeat(16_385)];
    for value in cases {
        let error = SecretString::new(value).expect_err("invalid secret");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }
    assert!(serde_json::from_str::<SecretString>("\"line\\nbreak\"").is_err());
    assert!(serde_json::from_str::<SecretString>("42").is_err());
}

#[test]
fn generated_credentials_have_fixed_valid_shapes() {
    for (kind, prefix) in [
        (GeneratedSecretKind::AccessToken, "bya"),
        (GeneratedSecretKind::ApiToken, "byd_pat"),
        (GeneratedSecretKind::MachineToken, "byd_ci"),
        (GeneratedSecretKind::BootstrapToken, "byb"),
        (GeneratedSecretKind::RuntimeSecret, "bys"),
        (GeneratedSecretKind::DownloadCapability, "byd"),
        (GeneratedSecretKind::ShareCapability, "bysh"),
        (GeneratedSecretKind::InboxCapability, "byin"),
        (GeneratedSecretKind::UploadCapability, "byu"),
    ] {
        let secret = SecretString::from_generated_entropy(kind, [0xab; 32]);
        assert_eq!(
            secret.expose_secret(),
            format!("{prefix}_{}", "ab".repeat(32))
        );
        assert_eq!(
            SecretString::new(secret.expose_secret()).expect("valid generated credential"),
            secret
        );
    }
}

#[test]
fn preview_host_capabilities_are_dns_safe_and_retain_all_entropy() {
    let zero = SecretString::from_preview_host_entropy([0; 32]);
    let ones = SecretString::from_preview_host_entropy([0xff; 32]);

    assert_eq!(zero.expose_secret(), "a".repeat(52));
    assert_eq!(ones.expose_secret(), format!("{}q", "7".repeat(51)));
    for capability in [zero, ones] {
        let value = capability.expose_secret();
        assert_eq!(value.len(), 52);
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
        );
        assert_eq!(
            SecretString::new(value).expect("valid capability"),
            capability
        );
    }
}

#[test]
fn slugs_support_all_conversion_and_serde_paths() {
    let slug = Slug::new("team_alpha-2").expect("valid slug");
    assert_eq!(slug.as_str(), "team_alpha-2");
    assert_eq!(slug.to_string(), "team_alpha-2");
    assert_eq!(Slug::from_str("team_alpha-2"), Ok(slug.clone()));
    assert_eq!(Slug::try_from("team_alpha-2".to_owned()), Ok(slug.clone()));
    assert_eq!(String::from(slug.clone()), "team_alpha-2");
    assert_eq!(
        serde_json::to_string(&slug).expect("serialize slug"),
        "\"team_alpha-2\""
    );
    assert_eq!(
        serde_json::from_str::<Slug>("\"team_alpha-2\"").expect("deserialize slug"),
        slug
    );
}

#[test]
fn slugs_reject_every_invalid_shape() {
    let cases = [
        String::new(),
        "-edge".into(),
        "edge_".into(),
        "two words".into(),
        "é".into(),
        "a".repeat(64),
    ];
    for value in cases {
        let error = Slug::new(value).expect_err("invalid slug");
        assert!(!error.to_string().is_empty());
    }
    assert!(serde_json::from_str::<Slug>("\"bad slug\"").is_err());
}

#[test]
fn uri_round_trips_through_its_canonical_string_serde_form() {
    let uri = BlobyardUri::from_str("blobyard://team/project/path/file.txt?version=2")
        .expect("valid uri");
    let encoded = serde_json::to_string(&uri).expect("serialize uri");
    assert_eq!(uri.workspace_slug().as_str(), "team");
    assert_eq!(uri.project_slug().as_str(), "project");
    assert_eq!(
        encoded,
        "\"blobyard://team/project/path/file.txt?version=2\""
    );
    assert_eq!(
        serde_json::from_str::<BlobyardUri>(&encoded).expect("deserialize uri"),
        uri
    );
    assert!(serde_json::from_str::<BlobyardUri>("\"invalid\"").is_err());
    assert!(serde_json::from_str::<BlobyardUri>("42").is_err());
}

#[test]
fn every_error_code_has_stable_actionable_default_copy_and_serde() {
    let codes = [
        ErrorCode::InvalidRequest,
        ErrorCode::AuthRequired,
        ErrorCode::InvalidToken,
        ErrorCode::TokenExpired,
        ErrorCode::Forbidden,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::PlanLimit,
        ErrorCode::OperationUnsupported,
        ErrorCode::UploadIncomplete,
        ErrorCode::ChecksumMismatch,
        ErrorCode::RateLimited,
        ErrorCode::ProviderUnavailable,
        ErrorCode::NetworkError,
        ErrorCode::StorageError,
        ErrorCode::InternalError,
        ErrorCode::Interrupted,
    ];
    for code in codes {
        let error = blobyard_core::BlobyardError::from_code(code);
        assert_eq!(error.message(), code.default_message());
        assert!(error.message().contains(['.', '!']));
        let json = serde_json::to_string(&code).expect("serialize code");
        assert_eq!(
            serde_json::from_str::<ErrorCode>(&json).expect("deserialize code"),
            code
        );
    }
}
