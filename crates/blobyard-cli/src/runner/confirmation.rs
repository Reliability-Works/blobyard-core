#![allow(
    clippy::redundant_pub_crate,
    reason = "the crate-local test runner injects deterministic confirmation behavior"
)]

use blobyard_core::{BlobyardError, ErrorCode};
use std::io::{BufRead, IsTerminal, Write};

pub(super) trait ConfirmationPort: Send + Sync {
    fn is_interactive(&self) -> bool;
    fn confirm(&self, prompt: &str) -> Result<bool, BlobyardError>;
}

pub(super) struct SystemConfirmation;

#[cfg(test)]
pub(crate) struct FixedConfirmation {
    pub(crate) interactive: bool,
    pub(crate) result: Result<bool, BlobyardError>,
}

impl ConfirmationPort for SystemConfirmation {
    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn confirm(&self, prompt: &str) -> Result<bool, BlobyardError> {
        confirm_with(
            &mut std::io::stdin().lock(),
            &mut std::io::stderr().lock(),
            prompt,
        )
    }
}

#[cfg(test)]
impl ConfirmationPort for FixedConfirmation {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn confirm(&self, _prompt: &str) -> Result<bool, BlobyardError> {
        self.result.clone()
    }
}

fn confirm_with(
    reader: &mut dyn BufRead,
    writer: &mut dyn Write,
    prompt: &str,
) -> Result<bool, BlobyardError> {
    writer
        .write_all(prompt.as_bytes())
        .and_then(|()| writer.flush())
        .map_err(local_confirmation_error)?;
    let mut answer = String::new();
    reader
        .read_line(&mut answer)
        .map_err(local_confirmation_error)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn local_confirmation_error<E>(_error: E) -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InternalError,
        "Blobyard couldn't read the confirmation. Re-run with the explicit confirmation flag.",
    )
}

#[cfg(test)]
mod tests {
    use super::{ConfirmationPort, SystemConfirmation, confirm_with};
    use std::io::{self, BufRead, Read, Write};

    #[test]
    fn accepts_only_explicit_yes_and_writes_the_prompt() {
        for (answer, expected) in [
            ("yes\n", true),
            ("Y\n", true),
            ("no\n", false),
            ("\n", false),
        ] {
            let mut output = Vec::new();
            assert_eq!(
                confirm_with(&mut answer.as_bytes(), &mut output, "Continue? [y/N] "),
                Ok(expected)
            );
            assert_eq!(output, b"Continue? [y/N] ");
        }
    }

    #[test]
    fn maps_read_and_write_failures_and_observes_terminal_state() {
        let mut no_input = std::io::empty();
        let mut no_output = std::io::sink();
        assert_eq!(
            confirm_with(&mut no_input, &mut no_output, "Prompt"),
            Ok(false)
        );
        let _terminal = SystemConfirmation.is_interactive();
        assert_eq!(SystemConfirmation.confirm("Prompt"), Ok(false));

        let mut broken = BrokenIo;
        assert!(confirm_with(&mut io::empty(), &mut broken, "Prompt").is_err());
        assert!(confirm_with(&mut broken, &mut io::sink(), "Prompt").is_err());
        let mut buffer = [0_u8; 1];
        assert!(broken.read(&mut buffer).is_err());
        broken.consume(0);
        assert!(broken.flush().is_err());
    }

    struct BrokenIo;

    impl Read for BrokenIo {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic"))
        }
    }

    impl BufRead for BrokenIo {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("synthetic"))
        }

        fn consume(&mut self, _amount: usize) {}
    }

    impl Write for BrokenIo {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("synthetic"))
        }
    }
}
