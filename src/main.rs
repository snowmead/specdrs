use std::env;
use std::process::ExitCode;

/// Runs the `specdrs` process.
fn main() -> ExitCode {
    specdrs::run_cli(env::args().skip(1))
}
