use super::{GITHUB_OIDC_ISSUER, OidcVerificationError};
use blobyard_contract::{
    GithubOidcIdentity, valid_github_ref_tail, valid_github_repository_part,
    valid_github_workflow_path,
};
use percent_encoding::percent_decode_str;
use serde::Deserialize;

const CLOCK_TOLERANCE_MS: u64 = 5 * 1_000;
const MAX_TOKEN_AGE_MS: u64 = 10 * 60 * 1_000;
const MAX_TOKEN_WINDOW_SECONDS: u64 = 15 * 60;

#[derive(Deserialize)]
pub(super) struct GithubClaims {
    aud: String,
    environment: Option<String>,
    exp: u64,
    iat: u64,
    iss: String,
    nbf: u64,
    #[serde(rename = "ref")]
    git_ref: String,
    repository: String,
    repository_owner: String,
    run_attempt: Option<String>,
    run_id: String,
    sha: Option<String>,
    sub: String,
    workflow_ref: String,
}

pub(super) fn identity(
    claims: GithubClaims,
    audience: &str,
    now_ms: u64,
) -> Result<GithubOidcIdentity, OidcVerificationError> {
    if claims.aud != audience || claims.iss != GITHUB_OIDC_ISSUER {
        return invalid();
    }
    let (repository, owner) = normalize_repository(&claims.repository)?;
    if claims.repository_owner.len() > 100
        || !valid_text(&claims.repository_owner)
        || claims.repository_owner.to_ascii_lowercase() != owner
    {
        return invalid();
    }
    validate_subject(&claims.sub, &repository)?;
    let (workflow_path, workflow_ref) = workflow_identity(&repository, &claims.workflow_ref)?;
    let environment = environment(&claims.sub, claims.environment.as_deref())?;
    let expires_at_ms = validate_times(&claims, now_ms)?;
    validate_ref(&claims.git_ref)?;
    validate_optional(claims.run_attempt.as_deref(), 20, valid_text)?;
    validate_optional(claims.sha.as_deref(), 40, valid_sha)?;
    validate_required(&claims.run_id, 100, valid_text)?;
    Ok(GithubOidcIdentity {
        audience: audience.to_owned(),
        repository,
        git_ref: claims.git_ref,
        workflow_path,
        workflow_ref,
        environment,
        run_id: claims.run_id,
        run_attempt: claims.run_attempt,
        sha: claims.sha,
        expires_at_ms,
    })
}

fn validate_times(claims: &GithubClaims, now_ms: u64) -> Result<u64, OidcVerificationError> {
    if claims.iat > claims.exp
        || claims.nbf > claims.exp
        || claims.exp.saturating_sub(claims.iat) > MAX_TOKEN_WINDOW_SECONDS
    {
        return Err(OidcVerificationError::Invalid);
    }
    let exp = claims
        .exp
        .checked_mul(1_000)
        .ok_or(OidcVerificationError::Invalid)?;
    // The ordering checks above prove `iat` and `nbf` cannot exceed `exp`, whose
    // multiplication was checked, so both products are bounded.
    let iat = claims.iat * 1_000;
    let nbf = claims.nbf * 1_000;
    if exp <= now_ms
        || iat > now_ms.saturating_add(CLOCK_TOLERANCE_MS)
        || nbf > now_ms.saturating_add(CLOCK_TOLERANCE_MS)
        || now_ms > iat.saturating_add(MAX_TOKEN_AGE_MS + CLOCK_TOLERANCE_MS)
    {
        invalid()
    } else {
        Ok(exp)
    }
}

fn workflow_identity(
    repository: &str,
    value: &str,
) -> Result<(String, String), OidcVerificationError> {
    let prefix = format!("{repository}/");
    let separator = value.rfind('@').ok_or(OidcVerificationError::Invalid)?;
    if separator <= prefix.len()
        || !value
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(&prefix))
    {
        return invalid();
    }
    let path = &value[prefix.len()..separator];
    let git_ref = &value[separator + 1..];
    validate_workflow_path(path)?;
    validate_ref(git_ref)?;
    Ok((path.to_owned(), git_ref.to_owned()))
}

fn validate_subject(subject: &str, repository: &str) -> Result<(), OidcVerificationError> {
    let source = subject
        .strip_prefix("repo:")
        .ok_or(OidcVerificationError::Invalid)?;
    let (subject_repository, _qualifier) = source
        .split_once(':')
        .ok_or(OidcVerificationError::Invalid)?;
    let (owner, name) = subject_repository
        .split_once('/')
        .ok_or(OidcVerificationError::Invalid)?;
    let (owner, owner_has_id) = subject_name(owner)?;
    let (name, name_has_id) = subject_name(name)?;
    if owner_has_id != name_has_id {
        return invalid();
    }
    let (normalized, _owner) = normalize_repository(&format!("{owner}/{name}"))?;
    if normalized == repository {
        Ok(())
    } else {
        invalid()
    }
}

fn subject_name(value: &str) -> Result<(&str, bool), OidcVerificationError> {
    if let Some((name, id)) = value.rsplit_once('@') {
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(OidcVerificationError::Invalid);
        }
        Ok((name, true))
    } else {
        Ok((value, false))
    }
}

fn environment(
    subject: &str,
    claimed: Option<&str>,
) -> Result<Option<String>, OidcVerificationError> {
    let marker = ":environment:";
    let from_subject = subject.find(marker).map(|index| {
        percent_decode_str(&subject[index + marker.len()..])
            .decode_utf8()
            .map(std::borrow::Cow::into_owned)
            .map_err(|_error| OidcVerificationError::Invalid)
    });
    let from_subject = from_subject.transpose()?;
    if claimed.is_some()
        && from_subject
            .as_deref()
            .is_some_and(|value| Some(value) != claimed)
    {
        return invalid();
    }
    let value = claimed.map(str::to_owned).or(from_subject);
    validate_optional(value.as_deref(), 100, valid_text)?;
    Ok(value)
}

fn normalize_repository(value: &str) -> Result<(String, String), OidcVerificationError> {
    let normalized = value.to_ascii_lowercase();
    let (owner, name) = normalized
        .split_once('/')
        .ok_or(OidcVerificationError::Invalid)?;
    if valid_github_repository_part(owner, 39, false)
        && valid_github_repository_part(name, 100, true)
    {
        let owner = owner.to_owned();
        Ok((normalized, owner))
    } else {
        invalid()
    }
}

fn validate_workflow_path(value: &str) -> Result<(), OidcVerificationError> {
    if valid_github_workflow_path(value) {
        Ok(())
    } else {
        invalid()
    }
}

fn validate_ref(value: &str) -> Result<(), OidcVerificationError> {
    let named = ["refs/heads/", "refs/tags/"].iter().any(|prefix| {
        value
            .strip_prefix(prefix)
            .is_some_and(valid_github_ref_tail)
    });
    let pull = value.strip_prefix("refs/pull/").is_some_and(valid_pull_ref);
    if named || pull || valid_sha(value) {
        Ok(())
    } else {
        invalid()
    }
}

fn valid_pull_ref(value: &str) -> bool {
    let Some((number, kind)) = value.split_once('/') else {
        return false;
    };
    !number.is_empty()
        && !number.starts_with('0')
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(kind, "head" | "merge")
}

fn valid_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn validate_required(
    value: &str,
    maximum: usize,
    validate: fn(&str) -> bool,
) -> Result<(), OidcVerificationError> {
    if value.len() <= maximum && validate(value) {
        Ok(())
    } else {
        invalid()
    }
}

fn validate_optional(
    value: Option<&str>,
    maximum: usize,
    validate: fn(&str) -> bool,
) -> Result<(), OidcVerificationError> {
    value
        .map(|value| validate_required(value, maximum, validate))
        .transpose()
        .map(|_value| ())
}

const fn invalid<T>() -> Result<T, OidcVerificationError> {
    Err(OidcVerificationError::Invalid)
}

#[cfg(test)]
#[path = "oidc_claims_tests.rs"]
mod tests;
