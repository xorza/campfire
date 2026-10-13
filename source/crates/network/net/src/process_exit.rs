use std::process::ExitCode;

use bevy_app::AppExit;
use campfire_common::ExitStatus;

/// How a binary's process exits once its app did.
#[derive(Debug)]
pub struct ProcessExit;

impl ProcessExit {
    /// The process's exit code for how the app exited.
    pub fn code(exit: AppExit) -> ExitCode {
        match exit {
            AppExit::Success => ExitCode::from(ExitStatus::Success),
            AppExit::Error(code) => ExitCode::from(code.get()),
        }
    }
}
