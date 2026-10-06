use std::panic;
use std::thread::{self, JoinHandle};

/// A named thread that runs one job until its owner ends it. The owner's `Drop` tells the job to
/// close, through the job's own wake: a flag and a condition variable, or a channel it drops.
/// The worker, a field of the owner, drops after that and joins the thread, and a panic of the
/// thread passes on to the owner.
#[derive(Debug)]
pub struct Worker {
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread `name`, which runs `job`.
    pub fn start(name: &str, job: impl FnOnce() + Send + 'static) -> Worker {
        let thread = thread::Builder::new()
            .name(name.to_owned())
            .spawn(job)
            .expect("the OS starts a thread");
        Worker {
            thread: Some(thread),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take()
            && let Err(payload) = thread.join()
        {
            panic::resume_unwind(payload);
        }
    }
}
