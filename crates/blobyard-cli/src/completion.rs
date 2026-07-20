use crate::{Cli, CompletionShell};
use clap::CommandFactory;
use clap_complete::{Shell, generate};

/// Generates an installable completion script for a supported shell.
#[must_use]
pub fn generate_completion(shell: CompletionShell) -> String {
    let shell = match shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    let mut bytes = Vec::new();
    generate(shell, &mut Cli::command(), "blobyard", &mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}
