use std::mem;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};

use crate::worker::Worker;
use crate::worker::worker_lock::WorkerLock;

/// One job at a time on a worker: the caller fills a job and sends it, and the worker does it and
/// gives it back with its answer, so the next job reuses its buffers and a steady state allocates
/// nothing. A send and a take never wait for the worker; only `wait` does. A worker that panicked
/// passes its panic on to the call that next needs it. Dropping the exchange lets the worker
/// finish the job it holds, then ends it.
#[derive(Debug)]
pub struct Exchange<J, A> {
    /// Dropped to close the worker.
    jobs: Option<SyncSender<J>>,
    done: WorkerLock<Receiver<Done<J, A>>>,
    /// The job the worker gave back last, for the next.
    spare: J,
    pending: bool,
    /// Dropped after the exchange's `Drop` closes it.
    worker: Worker,
}

/// A job the worker did, and its answer.
#[derive(Debug)]
struct Done<J, A> {
    job: J,
    answer: A,
}

impl<J: Default + Send + 'static, A: Send + 'static> Exchange<J, A> {
    /// Starts the worker `name`, which does each job with `work`.
    pub fn start(name: &str, mut work: impl FnMut(&mut J) -> A + Send + 'static) -> Self {
        // One job is out at a time, so a channel of one slot each way never makes a send wait,
        // and, bounded, holds its slot in a buffer it made once.
        let (jobs, received) = mpsc::sync_channel::<J>(1);
        let (sent, done) = mpsc::sync_channel(1);
        let worker = Worker::start(name, move || {
            while let Ok(mut job) = received.recv() {
                let answer = work(&mut job);
                if sent.send(Done { job, answer }).is_err() {
                    return;
                }
            }
        });
        Self {
            jobs: Some(jobs),
            done: WorkerLock::new(done),
            spare: J::default(),
            pending: false,
            worker,
        }
    }

    /// Sends the job `fill` makes of the one given back last, whose buffers it keeps.
    pub fn send(&mut self, fill: impl FnOnce(&mut J)) {
        assert!(!self.pending, "one job at a time");
        let mut job = mem::take(&mut self.spare);
        fill(&mut job);
        self.give(job);
    }

    /// Sends `job`, which the caller filled beforehand, and gives back the job given back last,
    /// whose buffers the caller keeps for the next it fills.
    pub fn send_filled(&mut self, job: J) -> J {
        assert!(!self.pending, "one job at a time");
        self.give(job);
        mem::take(&mut self.spare)
    }

    /// Gives `job` to the worker.
    fn give(&mut self, job: J) {
        let jobs = self
            .jobs
            .as_ref()
            .expect("an exchange closes only as it drops");
        if jobs.send(job).is_err() {
            self.worker.resume_panic();
        }
        self.pending = true;
    }

    /// Whether a job was sent and its answer not taken.
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// The answer to the job sent, once the worker gave it.
    pub fn take(&mut self) -> Option<A> {
        if !self.pending {
            return None;
        }
        let done = match self.done.get_mut().try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => self.worker.resume_panic(),
        };
        Some(self.keep(done))
    }

    /// The answer to the job sent, waiting for it; none when no job was sent.
    pub fn wait(&mut self) -> Option<A> {
        if !self.pending {
            return None;
        }
        let Ok(done) = self.done.get_mut().recv() else {
            self.worker.resume_panic();
        };
        Some(self.keep(done))
    }

    fn keep(&mut self, done: Done<J, A>) -> A {
        self.pending = false;
        self.spare = done.job;
        done.answer
    }
}

impl<J, A> Drop for Exchange<J, A> {
    fn drop(&mut self) {
        drop(self.jobs.take());
    }
}

#[cfg(test)]
mod tests;
