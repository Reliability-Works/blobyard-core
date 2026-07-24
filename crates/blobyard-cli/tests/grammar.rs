//! End-to-end command grammar and help contract tests.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_cli::{Cli, CompletionShell, generate_completion};
use clap::{CommandFactory, Parser};

include!("grammar/local_users.rs");

const COMPLETE_COMMANDS: &[&[&str]] = &[
    &["blobyard", "app", "init"],
    &["blobyard", "app", "validate"],
    &["blobyard", "app", "validate", "manifest.toml"],
    &[
        "blobyard",
        "profiles",
        "add",
        "local",
        "--api-url",
        "http://localhost:8787",
        "--token-stdin",
    ],
    &["blobyard", "login", "--name", "workstation", "--no-open"],
    &["blobyard", "logout"],
    &["blobyard", "whoami"],
    &[
        "blobyard",
        "init",
        "--workspace",
        "studio",
        "--project",
        "default",
    ],
    &["blobyard", "projects", "list"],
    &["blobyard", "projects", "create", "Mobile Builds"],
    &["blobyard", "workspaces", "list"],
    &["blobyard", "workspaces", "create", "Product Team"],
    &[
        "blobyard",
        "workspaces",
        "rename",
        "Platform Team",
        "--workspace",
        "product-team",
    ],
    &["blobyard", "billing", "checkout", "solo"],
    &["blobyard", "billing", "checkout", "team", "--seats", "5"],
    &["blobyard", "billing", "portal"],
    &["blobyard", "account", "export", "request"],
    &["blobyard", "account", "delete", "prepare"],
    &[
        "blobyard",
        "account",
        "delete",
        "complete",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--force",
        "--retry-key",
        "account-delete-20260715",
    ],
    &["blobyard", "upload", "./dist", "--path", "builds/main"],
    &[
        "blobyard",
        "download",
        "blobyard://studio/default/app.zip",
        "--output",
        "./app.zip",
        "--force",
    ],
    &[
        "blobyard",
        "ls",
        "blobyard://studio/default/builds",
        "--versions",
    ],
    &["blobyard", "rm", "blobyard://studio/default/old.zip"],
    &[
        "blobyard",
        "share",
        "./app.zip",
        "--expires",
        "7d",
        "--notify",
        "dev@example.com",
    ],
    &["blobyard", "preview", "./dist", "--expires", "24h"],
    &["blobyard", "shares", "list"],
    &["blobyard", "shares", "revoke", "share_123"],
    &["blobyard", "previews", "list"],
    &["blobyard", "previews", "revoke", "preview_123"],
    &[
        "blobyard",
        "deploy",
        "./dist",
        "--yard",
        "docs",
        "--spa",
        "--clean-urls",
        "--public",
    ],
    &[
        "blobyard",
        "deploy",
        "--all",
        "--spa",
        "--clean-urls",
        "--public",
    ],
    &["blobyard", "yard", "list"],
    &["blobyard", "yard", "show"],
    &["blobyard", "yard", "show", "docs"],
    &["blobyard", "yard", "history", "docs"],
    &["blobyard", "env", "list"],
    &["blobyard", "env", "list", "docs"],
    &["blobyard", "access", "list"],
    &["blobyard", "access", "list", "docs"],
    &["blobyard", "access", "set-visibility", "docs", "owner"],
    &[
        "blobyard",
        "access",
        "grant",
        "docs",
        "--principal-kind",
        "user",
        "--principal-id",
        "user_reader",
        "--role",
        "viewer",
        "--role",
        "editor",
        "--environment",
        "yardenv_docs",
        "--expires",
        "2100-01-01T00:00:00Z",
    ],
    &["blobyard", "access", "revoke", "docs", "yardgrant_1"],
    &["blobyard", "yard", "rollback", "docs", "deploy_1"],
    &["blobyard", "yard", "delete", "docs", "--force"],
    &[
        "blobyard",
        "inbox",
        "create",
        "Client logs",
        "--expires",
        "24h",
    ],
    &["blobyard", "inbox", "list"],
    &["blobyard", "inbox", "revoke", "inbox_123"],
    &[
        "blobyard",
        "retention",
        "set",
        "--latest",
        "20",
        "--branch",
        "main",
        "--path",
        "builds/**",
    ],
    &["blobyard", "retention", "show"],
    &["blobyard", "retention", "clear"],
    &["blobyard", "audit", "list", "--cursor", "cursor_2"],
    &["blobyard", "members", "list"],
    &["blobyard", "members", "role", "user_1", "--role", "admin"],
    &["blobyard", "members", "remove", "user_1", "--force"],
    &["blobyard", "invites", "list"],
    &[
        "blobyard",
        "invites",
        "create",
        "dev@example.com",
        "--role",
        "member",
    ],
    &["blobyard", "invites", "revoke", "invite_1"],
    &[
        "blobyard",
        "tokens",
        "create",
        "CI",
        "--expires-days",
        "7",
        "--scope",
        "audit:read",
    ],
    &["blobyard", "tokens", "list"],
    &["blobyard", "tokens", "revoke", "token_1"],
    &[
        "blobyard",
        "trusts",
        "create",
        "--repository",
        "owner/repo",
        "--workflow-path",
        ".github/workflows/release.yml",
        "--workflow-ref",
        "refs/heads/main",
        "--allowed-ref-glob",
        "refs/tags/*",
        "--action",
        "upload",
    ],
    &["blobyard", "trusts", "list"],
    &["blobyard", "trusts", "revoke", "trust_1"],
    &["blobyard", "sessions", "list"],
    &["blobyard", "sessions", "revoke", "session_1"],
    &["blobyard", "completion", "zsh"],
];

#[test]
fn accepts_the_complete_command_grammar() {
    for args in COMPLETE_COMMANDS.iter().chain(LOCAL_USER_COMMANDS) {
        assert!(
            Cli::try_parse_from(*args).is_ok(),
            "failed grammar: {args:?}"
        );
    }
}

#[test]
fn accepts_global_flags_after_nested_subcommands() {
    let result = Cli::try_parse_from([
        "blobyard",
        "projects",
        "list",
        "--json",
        "--api-url",
        "http://localhost:3210/v1",
        "--profile",
        "local",
        "--workspace",
        "studio",
        "--project",
        "default",
        "--retry-key",
        "nested-retry",
    ]);

    assert!(result.is_ok());
}

#[test]
fn root_help_names_every_command_and_global_flag() {
    let help = Cli::command().render_long_help().to_string();
    let expected = [
        "app",
        "profiles",
        "login",
        "logout",
        "whoami",
        "init",
        "workspaces",
        "billing",
        "account",
        "projects",
        "upload",
        "download",
        "ls",
        "rm",
        "share",
        "shares",
        "preview",
        "previews",
        "deploy",
        "yard",
        "env",
        "access",
        "inbox",
        "retention",
        "audit",
        "members",
        "invites",
        "tokens",
        "users",
        "trusts",
        "sessions",
        "completion",
        "--json",
        "--quiet",
        "--verbose",
        "--api-url",
        "--profile",
        "--workspace",
        "--project",
        "--retry-key",
    ];

    for item in expected {
        assert!(help.contains(item), "help omitted {item}");
    }
}

#[test]
fn generates_each_supported_shell_completion_contract() {
    let cases = [
        (CompletionShell::Bash, "bash"),
        (CompletionShell::Zsh, "zsh"),
        (CompletionShell::Fish, "fish"),
    ];
    for (shell, label) in cases {
        assert_eq!(shell.to_string(), label);
        let script = generate_completion(shell);
        assert!(script.contains("blobyard"));
        assert!(!script.is_empty());
    }
}
