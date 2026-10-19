use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

use campfire_common::ErrorReport;
use tracing::error;

use crate::error::CheckError;
use crate::process::Process;
use crate::process_outcome::ProcessOutcome;

/// How often the check looks at the processes while it waits.
pub(crate) const POLL: Duration = Duration::from_millis(50);

/// The processes a run started, each by its role, and how each ended that the check killed to
/// start another in its place: whatever way the run ends, an error that returns early among them,
/// each that still runs is stopped and waited for, so no process outlives the check.
#[derive(Debug, Default)]
pub(crate) struct ProcessGroup {
    running: BTreeMap<Process, Child>,
    replaced: BTreeMap<Process, ProcessOutcome>,
}

impl ProcessGroup {
    /// Adds `child`, started as `process`, a role no other process of the group has had.
    pub(crate) fn start(&mut self, process: Process, child: Child) {
        debug_assert!(
            !self.running.contains_key(&process) && !self.replaced.contains_key(&process),
            "{process} starts once"
        );
        self.running.insert(process, child);
    }

    /// How `process` ended; none while it runs.
    pub(crate) fn try_wait(
        &mut self,
        process: Process,
    ) -> Result<Option<ProcessOutcome>, CheckError> {
        let status = self
            .child(process)
            .try_wait()
            .map_err(|error| CheckError::Wait { process, error })?;
        Ok(status.map(ProcessOutcome::of))
    }

    /// Stops `process`, which overran the deadline.
    pub(crate) fn stop(&mut self, process: Process) -> Result<ProcessOutcome, CheckError> {
        stop(process, self.child(process))
    }

    /// Kills `old`, as a crash ends a process, and starts `child` as `new` in its place.
    pub(crate) fn replace(
        &mut self,
        old: Process,
        new: Process,
        child: impl FnOnce() -> Result<Child, CheckError>,
    ) -> Result<(), CheckError> {
        kill(old, self.child(old))?;
        self.running.remove(&old);
        self.replaced.insert(old, ProcessOutcome::Stopped);
        self.start(new, child()?);
        Ok(())
    }

    /// How each process ended, by its role, waiting until each did or `deadline` passed, when
    /// the check stops each that still runs; each it replaced was stopped.
    pub(crate) fn wait(
        &mut self,
        deadline: Instant,
    ) -> Result<BTreeMap<Process, ProcessOutcome>, CheckError> {
        let mut outcomes = self.replaced.clone();
        let all = self.replaced.len() + self.running.len();
        while outcomes.len() < all && Instant::now() < deadline {
            for (&process, child) in &mut self.running {
                if outcomes.contains_key(&process) {
                    continue;
                }
                let status = child
                    .try_wait()
                    .map_err(|error| CheckError::Wait { process, error })?;
                if let Some(status) = status {
                    outcomes.insert(process, ProcessOutcome::of(status));
                }
            }
            thread::sleep(POLL);
        }
        for (&process, child) in &mut self.running {
            if let Entry::Vacant(outcome) = outcomes.entry(process) {
                outcome.insert(stop(process, child)?);
            }
        }
        Ok(outcomes)
    }

    /// Stops each process that still runs, and waits for it; how many it stopped. A process it
    /// cannot stop does not keep it from the others: the error is the first such process's.
    pub(crate) fn stop_all(&mut self) -> Result<usize, CheckError> {
        let mut stopped = 0;
        let mut failed = None;
        for (&process, child) in &mut self.running {
            let ended = child
                .try_wait()
                .map_err(|error| CheckError::Wait { process, error })
                .and_then(|status| match status {
                    Some(_) => Ok(false),
                    None => kill(process, child).map(|()| true),
                });
            match ended {
                Ok(true) => stopped += 1,
                Ok(false) => {}
                Err(error) => {
                    failed.get_or_insert(error);
                }
            }
        }
        failed.map_or(Ok(stopped), Err)
    }

    fn child(&mut self, process: Process) -> &mut Child {
        self.running
            .get_mut(&process)
            .unwrap_or_else(|| panic!("{process} runs in the group"))
    }
}

/// A process the check cannot stop is one the operating system refuses it, and a drop has no
/// caller to tell; the run's own error, if any, is the one it reports.
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Err(error) = self.stop_all() {
            error!(error = %ErrorReport::of(&error), "a process of the run could not be stopped");
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
        group.start(Process::Server, sleeper(true));
        let mut done = sleeper(false);
        assert!(done.wait().unwrap().success());
        group.start(Process::Local, done);
        assert_eq!(group.try_wait(Process::Server).unwrap(), None);
        // One still runs: the group stops it, and waits for it; a second end stops none.
        assert_eq!(group.stop_all().unwrap(), 1);
        assert!(group.try_wait(Process::Server).unwrap().is_some());
        assert_eq!(group.stop_all().unwrap(), 0);
        // A replacement kills the process it replaces, so an end stops only the new one.
        let mut group = ProcessGroup::default();
        group.start(Process::Server, sleeper(true));
        group
            .replace(Process::Server, Process::ServerAgain, || Ok(sleeper(true)))
            .unwrap();
        assert_eq!(group.stop_all().unwrap(), 1);
        // How each ended, by its role: the one replaced stopped, the one that ran on overran.
        let mut group = ProcessGroup::default();
        group.start(Process::Server, sleeper(true));
        let mut done = sleeper(false);
        done.wait().unwrap();
        group.start(Process::Local, done);
        group
            .replace(Process::Server, Process::ServerAgain, || Ok(sleeper(true)))
            .unwrap();
        let outcomes = group.wait(Instant::now() + POLL).unwrap();
        assert_eq!(
            outcomes.into_iter().collect::<Vec<_>>(),
            [
                (Process::Server, ProcessOutcome::Stopped),
                (Process::ServerAgain, ProcessOutcome::Overran),
                (Process::Local, ProcessOutcome::Succeeded),
            ]
        );
    }
}
