use std::process::Child;
use std::thread;
use std::time::Instant;

use tracing::error;

use crate::error::CheckError;
use crate::lan_match::POLL;
use crate::process::Process;
use crate::process_outcome::ProcessOutcome;

/// The processes a run started, each with its role: whatever way the run ends, an error that
/// returns early among them, each that still runs is stopped and waited for, so no process
/// outlives the check.
#[derive(Debug, Default)]
pub(crate) struct ProcessGroup(Vec<(Process, Child)>);

impl ProcessGroup {
    /// Adds `child`, started as `process`; its place in the group.
    pub(crate) fn push(&mut self, process: Process, child: Child) -> usize {
        self.0.push((process, child));
        self.0.len() - 1
    }

    pub(crate) fn child_mut(&mut self, at: usize) -> &mut Child {
        &mut self.0[at].1
    }

    /// Stops the process at `at`, which overran the deadline.
    pub(crate) fn stop(&mut self, at: usize) -> Result<ProcessOutcome, CheckError> {
        let (process, child) = &mut self.0[at];
        stop(*process, child)
    }

    /// Kills the process at `at`, as a crash ends a process, and puts `child`, started as
    /// `process`, in its place.
    pub(crate) fn replace(
        &mut self,
        at: usize,
        process: Process,
        child: impl FnOnce() -> Result<Child, CheckError>,
    ) -> Result<(), CheckError> {
        let (old, running) = &mut self.0[at];
        kill(*old, running)?;
        self.0[at] = (process, child()?);
        Ok(())
    }

    /// How each process ended, waiting until each did or `deadline` passed, when the check stops
    /// each that still runs.
    pub(crate) fn wait(&mut self, deadline: Instant) -> Result<Vec<ProcessOutcome>, CheckError> {
        let mut outcomes = vec![None; self.0.len()];
        while outcomes.iter().any(Option::is_none) && Instant::now() < deadline {
            for ((process, child), outcome) in self.0.iter_mut().zip(&mut outcomes) {
                if outcome.is_none() {
                    *outcome = child
                        .try_wait()
                        .map_err(|error| CheckError::Wait {
                            process: *process,
                            error,
                        })?
                        .map(ProcessOutcome::of);
                }
            }
            thread::sleep(POLL);
        }
        self.0
            .iter_mut()
            .zip(outcomes)
            .map(|((process, child), outcome)| match outcome {
                Some(outcome) => Ok(outcome),
                None => stop(*process, child),
            })
            .collect()
    }

    /// Stops each process that still runs, and waits for it; how many it stopped.
    pub(crate) fn end(&mut self) -> Result<usize, CheckError> {
        let mut stopped = 0;
        for (process, child) in &mut self.0 {
            let running = child.try_wait().map_err(|error| CheckError::Wait {
                process: *process,
                error,
            })?;
            if running.is_none() {
                kill(*process, child)?;
                stopped += 1;
            }
        }
        Ok(stopped)
    }
}

/// A process the check cannot stop is one the operating system refuses it, and a drop has no
/// caller to tell; the run's own error, if any, is the one it reports.
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Err(error) = self.end() {
            error!(%error, "a process of the run could not be stopped");
        }
    }
}

/// Kills `child`, which overran the deadline.
fn stop(process: Process, child: &mut Child) -> Result<ProcessOutcome, CheckError> {
    kill(process, child)?;
    Ok(ProcessOutcome::Overran)
}

/// Kills `child`, as a crash ends a process, and waits for it.
fn kill(process: Process, child: &mut Child) -> Result<(), CheckError> {
    child
        .kill()
        .and_then(|()| child.wait())
        .map_err(|error| CheckError::Wait { process, error })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    /// Set, the test `sleeps_when_asked` sleeps a minute: a process that runs on.
    const SLEEP: &str = "CAMPFIRE_LAN_CHECK_SLEEP";

    #[test]
    fn sleeps_when_asked() {
        if env::var_os(SLEEP).is_some() {
            thread::sleep(Duration::from_secs(60));
        }
    }

    /// This test binary, running `sleeps_when_asked` alone, which sleeps if `sleep`.
    fn sleeper(sleep: bool) -> Child {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--exact", "process_group::tests::sleeps_when_asked"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if sleep {
            command.env(SLEEP, "1");
        }
        command.spawn().unwrap()
    }

    #[test]
    fn a_group_stops_what_still_runs_and_leaves_what_ended() {
        let mut group = ProcessGroup::default();
        let running = group.push(Process::Server, sleeper(true));
        let mut done = sleeper(false);
        assert!(done.wait().unwrap().success());
        let ended = group.push(Process::Local, done);
        assert_eq!((running, ended), (0, 1));
        // One still runs: the group stops it, and waits for it; a second end stops none.
        assert_eq!(group.end().unwrap(), 1);
        assert!(group.child_mut(running).try_wait().unwrap().is_some());
        assert_eq!(group.end().unwrap(), 0);
        // A replacement kills the process it replaces, so an end stops only the new one.
        let mut group = ProcessGroup::default();
        group.push(Process::Server, sleeper(true));
        group
            .replace(0, Process::ServerAgain, || Ok(sleeper(true)))
            .unwrap();
        assert_eq!(group.end().unwrap(), 1);
    }
}
