use super::{lifecycle_audit::to_i64, lifecycle_deletion, lifecycle_retention, map_error, rows};
use blobyard_contract::{DeletionPlan, RepositoryError, RetentionPolicyRecord};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};
use std::collections::HashMap;

pub(super) fn begin(
    transaction: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    actor: &str,
    request_id: &str,
    started_at_ms: u64,
) -> Result<DeletionPlan, RepositoryError> {
    for value in [project_id, run_id, actor, request_id] {
        rows::validate_text(value)?;
    }
    if let Some(existing) = pending_operation(transaction, project_id)? {
        resume_run(transaction, &existing)?;
        return lifecycle_deletion::deletion_plan(transaction, &existing);
    }
    let policy = lifecycle_retention::policy(transaction, project_id)?;
    let (matching, candidate_count) = matching_versions(transaction, &policy)?;
    let started_at_ms = to_i64(started_at_ms)?;
    insert_run(
        transaction,
        run_id,
        project_id,
        candidate_count,
        started_at_ms,
    )?;
    insert_operation(
        transaction,
        run_id,
        project_id,
        actor,
        request_id,
        started_at_ms,
    )?;
    for (index, candidate) in (0_u64..).zip(&matching) {
        if index < u64::from(policy.keep_latest) {
            continue;
        }
        transaction
            .execute(
                "INSERT INTO deletion_items (operation_id, version_id, storage_key, version) VALUES (?1, ?2, ?3, ?4)",
                params![run_id, candidate.id, candidate.storage_key, candidate.version],
            )
            .map_err(map_error)?;
    }
    lifecycle_deletion::deletion_plan(transaction, run_id)
}

#[derive(Debug)]
struct Candidate {
    id: String,
    path: String,
    storage_key: String,
    version: i64,
    git_branch: Option<String>,
}

fn matching_versions(
    connection: &Connection,
    policy: &RetentionPolicyRecord,
) -> Result<(Vec<Candidate>, i64), RepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT id, object_path, storage_key, version, git_branch FROM object_versions WHERE project_id = ?1 AND state = 'complete' ORDER BY created_at_ms DESC, id DESC",
        )
        .map_err(map_error)?;
    query_candidates(&mut statement, &policy.project_id).map(|candidates| {
        let mut matching = Vec::new();
        let mut candidate_count = 0_i64;
        for candidate in candidates {
            if !protected_path(&candidate.path) && policy_matches(policy, &candidate) {
                matching.push(candidate);
                candidate_count += 1;
            }
        }
        (matching, candidate_count)
    })
}

fn query_candidates(
    statement: &mut Statement<'_>,
    project_id: &str,
) -> Result<Vec<Candidate>, RepositoryError> {
    statement
        .query_map([project_id], candidate_row)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Candidate> {
    let version: i64 = row.get(3)?;
    u64::try_from(version).map_err(rows::conversion_error)?;
    Ok(Candidate {
        id: row.get(0)?,
        path: row.get(1)?,
        storage_key: row.get(2)?,
        version,
        git_branch: row.get(4)?,
    })
}

fn policy_matches(policy: &RetentionPolicyRecord, candidate: &Candidate) -> bool {
    let path = policy
        .path_glob
        .as_deref()
        .is_none_or(|glob| glob_matches(glob, &candidate.path));
    let branch = policy.branch_glob.as_deref().is_none_or(|glob| {
        candidate
            .git_branch
            .as_deref()
            .is_some_and(|value| glob_matches(glob, value))
    });
    path && branch
}

fn protected_path(path: &str) -> bool {
    [".blobyard-preview/", ".blobyard-yard/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn glob_matches(glob: &str, value: &str) -> bool {
    let pattern = glob.chars().collect::<Vec<_>>();
    let input = value.chars().collect::<Vec<_>>();
    glob_at(&pattern, &input, 0, 0, &mut HashMap::new())
}

fn glob_at(
    pattern: &[char],
    input: &[char],
    pattern_index: usize,
    input_index: usize,
    memo: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(value) = memo.get(&(pattern_index, input_index)) {
        return *value;
    }
    let result = match pattern.get(pattern_index) {
        None => input_index == input.len(),
        Some('*') if pattern.get(pattern_index + 1) == Some(&'*') => {
            glob_at(pattern, input, pattern_index + 2, input_index, memo)
                || (input_index < input.len()
                    && glob_at(pattern, input, pattern_index, input_index + 1, memo))
        }
        Some('*') => {
            glob_at(pattern, input, pattern_index + 1, input_index, memo)
                || (input
                    .get(input_index)
                    .is_some_and(|character| *character != '/')
                    && glob_at(pattern, input, pattern_index, input_index + 1, memo))
        }
        Some('?') => {
            input
                .get(input_index)
                .is_some_and(|character| *character != '/')
                && glob_at(pattern, input, pattern_index + 1, input_index + 1, memo)
        }
        Some(character) => {
            input.get(input_index) == Some(character)
                && glob_at(pattern, input, pattern_index + 1, input_index + 1, memo)
        }
    };
    memo.insert((pattern_index, input_index), result);
    result
}

fn pending_operation(
    connection: &Connection,
    project_id: &str,
) -> Result<Option<String>, RepositoryError> {
    connection
        .query_row(
            "SELECT id FROM deletion_operations WHERE project_id = ?1 AND reason = 'retention' AND status = 'pending'",
            [project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)
}

fn resume_run(transaction: &Transaction<'_>, id: &str) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "UPDATE retention_runs SET status = 'running', completed_at_ms = NULL, error_summary = NULL WHERE id = ?1 AND status != 'complete'",
            [id],
        )
        .map_err(map_error)?;
    Ok(())
}

fn insert_run(
    transaction: &Transaction<'_>,
    id: &str,
    project_id: &str,
    candidates: i64,
    started_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO retention_runs (id, project_id, candidate_count, deleted_count, status, started_at_ms) VALUES (?1, ?2, ?3, 0, 'running', ?4)",
            params![
                id,
                project_id,
                candidates,
                started_at_ms,
            ],
        )
        .map_err(map_error)?;
    Ok(())
}

fn insert_operation(
    transaction: &Transaction<'_>,
    id: &str,
    project_id: &str,
    actor: &str,
    request_id: &str,
    started_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO deletion_operations (id, project_id, object_path, selected_version, reason, status, actor, request_id, created_at_ms) VALUES (?1, ?2, '.retention', NULL, 'retention', 'pending', ?3, ?4, ?5)",
            params![id, project_id, actor, request_id, started_at_ms],
        )
        .map_err(map_error)?;
    Ok(())
}

#[cfg(test)]
#[path = "lifecycle_retention_plan_tests.rs"]
mod tests;
