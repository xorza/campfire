use std::panic;
use std::thread::{self, JoinHandle};

use crate::thread_name::ThreadName;

/// A named thread that runs one job until its holder ends it. The holder's `Drop` tells the job to
/// close, through the job's own wake: a flag and a condition variable, or a channel it drops.
/// The worker, a field of the holder, drops after that and joins the thread, and a panic of the
/// thread passes on to the holder. Dropped while its own thread unwinds a panic, it does not
/// join: a crash then ends the process at once, as a worker blocked on a disk that does not
/// answer would hold it, and what the worker had not synced is lost, as a crash loses it; nor
/// does it pass the worker's panic on, which would abort.
#[derive(Debug)]
pub struct Worker {
    thread: Option<JoinHandle<()>>,
    end: WorkerEnd,
}

/// What a worker's drop does with its thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerEnd {
    /// Joins it, once its holder told its job to close.
    Joined,
    /// Leaves it to the process's exit, as no holder can wake its wait.
    Left,
}

impl Worker {
    /// Starts the thread `name`, a job's, which runs `run`.
    pub fn start(name: impl Into<ThreadName>, run: impl FnOnce() + Send + 'static) -> Self {
        Self::spawn(name.into(), Box::new(run), WorkerEnd::Joined)
    }

    /// Starts the thread `name`, a job's, which runs `run`, a wait that no holder can wake, as a
    /// read of the standard input, which no OS wakes portably: its drop leaves the thread
    /// blocked, to end with the process (console.md, A reader of the standard input is left to
    /// the exit).
    pub fn start_left(name: impl Into<ThreadName>, run: impl FnOnce() + Send + 'static) -> Self {
        Self::spawn(name.into(), Box::new(run), WorkerEnd::Left)
    }

    fn spawn(name: ThreadName, run: Box<dyn FnOnce() + Send>, end: WorkerEnd) -> Self {
        let thread = thread::Builder::new()
            .name(name.as_str().to_owned())
            .spawn(run)
            .expect("the OS starts a thread");
        Self {
            thread: Some(thread),
            end,
        }
    }

    /// Passes on the panic of the thread when it ended while its holder still needs it, as a
    /// worker ends early only by a panic; nothing while it runs. A holder whose job's channel
    /// cannot tell it the thread ended calls it where it next needs the worker.
    pub fn check(&mut self) {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            self.resume_panic();
        }
    }

    /// Joins the thread, which ended while its holder still needed it, or which ends as it
    /// unwinds, and passes its panic on: a worker ends early only by a panic.
    pub fn resume_panic(&mut self) -> ! {
        let thread = self.thread.take().expect("a worker joins once");
        match thread.join() {
            Err(payload) => panic::resume_unwind(payload),
            Ok(()) => panic!("a worker ends only as its holder closes it, or by a panic"),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        if thread::panicking() || self.end == WorkerEnd::Left {
            return;
        }
        if let Err(payload) = thread.join() {
            panic::resume_unwind(payload);
        }
    }
}

#[cfg(test)]
mod tests;
