//! Repository-local quality checks that are awkward to express in standard linters.

use std::fs::{self, DirEntry, FileType};
use std::io;
use std::path::{Path, PathBuf};

/// Maximum nonblank, noncomment lines in a Rust source file.
pub const MAX_RUST_FILE_LINES: usize = 300;

const CHECK_LIMITS_COMMAND: &str = "check-limits";
const USAGE: &str = "usage: cargo run -p xtask -- check-limits\n";

/// A Rust source file that exceeds the repository size limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitViolation {
    path: PathBuf,
    actual_lines: usize,
    maximum_lines: usize,
}

impl LimitViolation {
    /// Returns the source path relative to the scanned workspace.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the counted nonblank, noncomment lines.
    #[must_use]
    pub const fn actual_lines(&self) -> usize {
        self.actual_lines
    }

    /// Returns the configured maximum line count.
    #[must_use]
    pub const fn maximum_lines(&self) -> usize {
        self.maximum_lines
    }
}

/// Captured output and process status for an xtask invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutcome {
    exit_code: u8,
    stdout: String,
    stderr: String,
}

impl CommandOutcome {
    /// Returns the process exit code.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// Returns content intended for standard output.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns content intended for standard error.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

/// Runs an xtask command against an explicit workspace root.
#[must_use]
pub fn run(arguments: &[String], workspace_root: &Path) -> CommandOutcome {
    if arguments != [CHECK_LIMITS_COMMAND] {
        return outcome(2, "", USAGE);
    }
    match check_limits(workspace_root) {
        Ok(violations) if violations.is_empty() => outcome(0, "Rust source limits passed.\n", ""),
        Ok(violations) => outcome(1, "", &format_violations(&violations)),
        Err(error) => outcome(
            1,
            "",
            &format!("failed to check Rust source limits: {error}\n"),
        ),
    }
}

fn outcome(exit_code: u8, stdout: &str, stderr: &str) -> CommandOutcome {
    CommandOutcome {
        exit_code,
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
    }
}

fn format_violations(violations: &[LimitViolation]) -> String {
    let lines = violations
        .iter()
        .map(|violation| {
            format!(
                "{} has {} nonblank/noncomment lines; maximum is {}",
                violation.path().display(),
                violation.actual_lines(),
                violation.maximum_lines()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{lines}\n")
}

/// Scans all Rust files under `crates/` for repository line limits.
///
/// Function length, cognitive complexity, and nesting are enforced by Clippy
/// using the thresholds in the root `clippy.toml`.
///
/// # Errors
///
/// Returns an I/O error when the workspace source tree cannot be traversed or
/// a Rust file cannot be read.
pub fn check_limits(workspace_root: &Path) -> io::Result<Vec<LimitViolation>> {
    let mut files = Vec::new();
    collect_rust_files(&workspace_root.join("crates"), &mut files)?;
    files.sort();
    let mut violations = Vec::new();
    for path in files {
        if let Some(violation) = check_file(workspace_root, &path)? {
            violations.push(violation);
        }
    }
    Ok(violations)
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = fs::read_dir(directory)?;
    collect_entries(entries, files)
}

fn collect_entries(
    entries: impl IntoIterator<Item = io::Result<DirEntry>>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in entries {
        collect_entry(entry, files)?;
    }
    Ok(())
}

fn collect_entry(entry: io::Result<DirEntry>, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entry = entry?;
    classify_path(entry.path(), entry.file_type(), files)
}

fn classify_path(
    path: PathBuf,
    file_type: io::Result<FileType>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let file_type = file_type?;
    if file_type.is_dir() {
        return collect_rust_files(&path, files);
    }
    if file_type.is_file() && path.extension().is_some_and(|value| value == "rs") {
        files.push(path);
    }
    Ok(())
}

fn check_file(workspace_root: &Path, path: &Path) -> io::Result<Option<LimitViolation>> {
    let source = fs::read_to_string(path)?;
    let actual_lines = count_source_lines(&source);
    if actual_lines <= MAX_RUST_FILE_LINES {
        return Ok(None);
    }
    let relative_path = path.strip_prefix(workspace_root).unwrap_or(path).to_owned();
    Ok(Some(LimitViolation {
        path: relative_path,
        actual_lines,
        maximum_lines: MAX_RUST_FILE_LINES,
    }))
}

fn count_source_lines(source: &str) -> usize {
    let mut block_depth = 0_u32;
    source
        .lines()
        .filter(|line| line_has_code(line, &mut block_depth))
        .count()
}

fn line_has_code(line: &str, block_depth: &mut u32) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let pair = bytes.get(index..index.saturating_add(2));
        if *block_depth > 0 {
            if pair == Some(b"/*") {
                *block_depth = block_depth.saturating_add(1);
                index += 2;
            } else if pair == Some(b"*/") {
                *block_depth = block_depth.saturating_sub(1);
                index += 2;
            } else {
                index += 1;
            }
        } else if bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        } else if pair == Some(b"//") {
            return false;
        } else if pair == Some(b"/*") {
            *block_depth = block_depth.saturating_add(1);
            index += 2;
        } else {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
