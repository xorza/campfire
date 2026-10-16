use std::panic;
use std::thread::{self, JoinHandle};

pub(crate) mod failure;
pub(crate) mod worker_lock;

/// A named thread that runs one job until its owner ends it. The owner's `Drop` tells the job to
/// close, through the job's own wake: a flag and a condition variable, or a channel it drops.
/// The worker, a field of the owner, drops after that and joins the thread, and a panic of the
/// thread passes on to the owner. Dropped while its own thread unwinds a panic, it does not
/// join: a crash then ends the process at once, as a worker blocked on a disk that does not
/// answer would hold it, and what the worker had not synced is lost, as a crash loses it; nor
/// does it pass the worker's panic on, which would abort.
#[derive(Debug)]
pub struct Worker {
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread `name`, which runs `job`.
    pub fn start(name: &str, job: impl FnOnce() + Send + 'static) -> Self {
        let thread = thread::Builder::new()
            .name(name.to_owned())
            .spawn(job)
            .expect("the OS starts a thread");
        Self {
            thread: Some(thread),
        }
    }

    /// Passes on the panic of the thread when it ended while its owner still needs it, as a
    /// worker ends early only by a panic; nothing while it runs. An owner whose job's channel
    /// cannot tell it the thread ended calls it where it next needs the worker.
    pub fn check(&mut self) {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            self.resume_panic();
        }
    }

    /// Joins the thread, which ended while its owner still needed it, or which ends as it
    /// unwinds, and passes its panic on: a worker ends early only by a panic.
    pub fn resume_panic(&mut self) -> ! {
        let thread = self.thread.take().expect("a worker joins once");
        match thread.join() {
            Err(payload) => panic::resume_unwind(payload),
            Ok(()) => panic!("a worker ends only as its owner closes it, or by a panic"),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        if thread::panicking() {
            return;
        }
        if let Err(payload) = thread.join() {
            panic::resume_unwind(payload);
        }
    }
}

#[cfg(test)]
mod tests;
