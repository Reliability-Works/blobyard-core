use crate::OutputMode;
use indicatif::ProgressBar;
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runner) struct TransferProgress {
    visible: bool,
}

impl TransferProgress {
    pub(in crate::runner) const fn hidden() -> Self {
        Self { visible: false }
    }

    pub(in crate::runner) fn for_output(mode: OutputMode) -> Self {
        Self::for_output_with_terminal(mode, std::io::stderr().is_terminal())
    }

    const fn for_output_with_terminal(mode: OutputMode, terminal: bool) -> Self {
        Self {
            visible: matches!(mode, OutputMode::Human) && terminal,
        }
    }

    pub(super) fn start(self, label: &str, size: u64) -> ProgressBar {
        let progress = if self.visible {
            ProgressBar::new(size)
        } else {
            ProgressBar::hidden()
        };
        progress.set_message(label.to_owned());
        progress
    }
}

#[cfg(test)]
mod tests {
    use super::TransferProgress;
    use crate::OutputMode;

    #[test]
    fn progress_selection_preserves_labels_and_suppresses_machine_modes() {
        let human = TransferProgress::for_output_with_terminal(OutputMode::Human, true)
            .start("artifact.bin", 42);
        assert_eq!(human.length(), Some(42));
        assert_eq!(human.message(), "artifact.bin");
        human.finish_and_clear();

        let modes = [
            (OutputMode::Human, false),
            (OutputMode::Json, true),
            (OutputMode::Quiet, true),
        ];
        for (mode, terminal) in modes {
            let progress =
                TransferProgress::for_output_with_terminal(mode, terminal).start("hidden", 7);
            assert!(progress.is_hidden());
        }
    }
}
