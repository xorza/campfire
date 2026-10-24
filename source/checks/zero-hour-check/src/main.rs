//! The Zero Hour install check's command, which `campfire_zero_hour_check::run` runs.

use std::process::ExitCode;

fn main() -> ExitCode {
    campfire_zero_hour_check::run()
}
