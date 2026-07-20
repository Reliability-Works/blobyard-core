use std::path::Path;
use std::process::Command;

const MAX_PROVENANCE_LENGTH: usize = 512;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct GitProvenance {
    pub(super) branch: Option<String>,
    pub(super) commit: Option<String>,
    pub(super) repository: Option<String>,
}

pub(super) fn discover(source: &Path) -> GitProvenance {
    let parent = source.with_file_name("");
    let directory = if source.is_dir() {
        source
    } else {
        parent.as_path()
    };
    if git_value(directory, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return GitProvenance::default();
    }
    GitProvenance {
        branch: git_value(directory, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .filter(|value| valid_reference(value)),
        commit: git_value(directory, &["rev-parse", "HEAD"]).filter(|value| valid_commit(value)),
        repository: git_value(directory, &["remote", "get-url", "origin"])
            .and_then(|value| repository_name(&value)),
    }
}

fn git_value(directory: &Path, arguments: &[&str]) -> Option<String> {
    command_value("git", directory, arguments)
}

fn command_value(program: &str, directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .ok()?;
    parse_output(output.status.success(), output.stdout)
}

fn parse_output(success: bool, stdout: Vec<u8>) -> Option<String> {
    if !success {
        return None;
    }
    let value = String::from_utf8(stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_PROVENANCE_LENGTH {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_reference(value: &str) -> bool {
    value.len() <= 255
        && !value.starts_with('-')
        && !value.contains("..")
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn repository_name(value: &str) -> Option<String> {
    let path = url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "https" | "ssh"))
        .and_then(|url| url.host_str().map(|_host| url.path().to_owned()))
        .or_else(|| scp_repository_path(value))?;
    normalize_repository_path(&path)
}

fn scp_repository_path(value: &str) -> Option<String> {
    let (authority, path) = value.split_once(':')?;
    (authority.contains('@') && !path.contains(':')).then(|| path.to_owned())
}

fn normalize_repository_path(value: &str) -> Option<String> {
    let trimmed = value
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| value.trim_matches('/'));
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 255
        && trimmed.contains('/')
        && !trimmed
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'));
    valid.then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

    use super::{
        command_value, discover, normalize_repository_path, parse_output, repository_name,
        valid_commit, valid_reference,
    };
    use std::path::Path;
    use std::process::Command;

    #[test]
    fn discovers_committed_git_provenance_without_remote_credentials() {
        let temp = tempfile::tempdir().expect("temp");
        let root = temp.path();
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.name", "Blobyard Test"]);
        git(root, &["config", "user.email", "test@blobyard.invalid"]);
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:blobyard/blobyard.git",
            ],
        );
        std::fs::write(root.join("artifact.bin"), b"artifact").expect("artifact");
        git(root, &["add", "artifact.bin"]);
        git(root, &["commit", "--quiet", "-m", "fixture"]);
        git(root, &["branch", "-M", "main"]);

        let provenance = discover(&root.join("artifact.bin"));
        assert_eq!(provenance.branch.as_deref(), Some("main"));
        assert_eq!(provenance.repository.as_deref(), Some("blobyard/blobyard"));
        assert!(
            provenance
                .commit
                .is_some_and(|commit| valid_commit(&commit))
        );

        git(root, &["checkout", "--quiet", "--detach"]);
        assert_eq!(discover(root).branch, None);
    }

    #[test]
    fn absent_or_unsafe_git_metadata_is_omitted() {
        let temp = tempfile::tempdir().expect("temp");
        assert_eq!(discover(temp.path()), super::GitProvenance::default());
    }

    #[test]
    fn command_and_output_failures_omit_provenance() {
        let temp = tempfile::tempdir().expect("temp");
        assert_eq!(
            command_value("blobyard-command-does-not-exist", temp.path(), &[]),
            None
        );
        assert_eq!(parse_output(false, b"ignored".to_vec()), None);
        assert_eq!(parse_output(true, vec![0xff]), None);
        assert_eq!(parse_output(true, b"\n".to_vec()), None);
        assert_eq!(parse_output(true, vec![b'x'; 513]), None);
        assert_eq!(normalize_repository_path(""), None);
    }

    #[test]
    fn repository_urls_are_normalized_without_credentials() {
        assert_eq!(
            repository_name("https://github.com/blobyard/blobyard.git").as_deref(),
            Some("blobyard/blobyard")
        );
        assert_eq!(
            repository_name("ssh://git@gitlab.com/team/tools/blobyard.git").as_deref(),
            Some("team/tools/blobyard")
        );
        assert_eq!(
            repository_name("git@git.example:team/blobyard.git").as_deref(),
            Some("team/blobyard")
        );
        for invalid in [
            "local",
            "file:///tmp/repo",
            "host:repo",
            "git@host:../repo",
            "git@host:team/re po",
        ] {
            assert_eq!(repository_name(invalid), None);
        }
    }

    #[test]
    fn commit_and_reference_validators_fail_closed() {
        assert!(valid_reference("feature/safe-name"));
        assert!(!valid_reference("-unsafe"));
        assert!(!valid_reference("unsafe..name"));
        assert!(!valid_reference(&"x".repeat(256)));
        assert!(!valid_reference("line\nbreak"));
        assert!(valid_commit(&"a".repeat(64)));
        assert!(!valid_commit("not-a-commit"));
    }

    fn git(directory: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(arguments)
            .status()
            .expect("git command");
        assert!(status.success());
    }
}
