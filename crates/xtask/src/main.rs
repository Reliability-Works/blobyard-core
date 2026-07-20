//! Blobyard repository task runner entry point.

use std::env;
use std::path::Path;
use std::process::ExitCode;
use xtask::run;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let outcome = run(&arguments, &root);
    print!("{}", outcome.stdout());
    eprint!("{}", outcome.stderr());
    ExitCode::from(outcome.exit_code())
}
