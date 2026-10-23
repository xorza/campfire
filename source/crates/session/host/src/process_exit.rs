use std::process::ExitCode;

use bevy_app::AppExit;
use campfire_common::ExitStatus;

/// The exit code of a binary's process for how its app exited.
pub fn exit_code(exit: AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::from(ExitStatus::Success),
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}
