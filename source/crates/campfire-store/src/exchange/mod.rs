use std::mem;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, SyncSender};

use crate::worker::Worker;

/// One job at a time on a worker: the caller fills a job and sends it, and the worker does it and
/// gives it back with its answer, so the next job reuses its buffers and a steady state allocates
/// nothing. A send and a take never wait for the worker; only `wait` does. Dropping the exchange
/// lets the worker finish the job it holds, then ends it.
#[derive(Debug)]
pub struct Exchange<J, A> {
    /// Dropped to close the worker.
    jobs: Option<SyncSender<J>>,
    done: Mutex<Receiver<Done<J, A>>>,
    /// The job the worker gave back last, for the next.
    spare: J,
    pending: bool,
    /// Dropped after the exchange's `Drop` closes it.
    _worker: Worker,
}

/// A job the worker did, and its answer.
#[derive(Debug)]
struct Done<J, A> {
    job: J,
    answer: A,
}

impl<J: Default + Send + 'static, A: Send + 'static> Exchange<J, A> {
    /// Starts the worker `name`, which does each job with `work`.
    pub fn start(name: &str, mut work: impl FnMut(&mut J) -> A + Send + 'static) -> Exchange<J, A> {
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
        Exchange {
            jobs: Some(jobs),
            done: Mutex::new(done),
            spare: J::default(),
            pending: false,
            _worker: worker,
        }
    }

    /// Sends the job `fill` makes of the one given back last, whose buffers it keeps.
    pub fn send(&mut self, fill: impl FnOnce(&mut J)) {
        assert!(!self.pending, "one job at a time");
        let mut job = mem::take(&mut self.spare);
        fill(&mut job);
        self.jobs
            .as_ref()
            .expect("an exchange closes only as it drops")
            .send(job)
            .expect("the worker runs");
        self.pending = true;
    }

    /// Sends `job`, which the caller filled beforehand, and gives back the job given back last,
    /// whose buffers the caller keeps for the next it fills.
    pub fn send_filled(&mut self, job: J) -> J {
        assert!(!self.pending, "one job at a time");
        self.jobs
            .as_ref()
            .expect("an exchange closes only as it drops")
            .send(job)
            .expect("the worker runs");
        self.pending = true;
        mem::take(&mut self.spare)
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
        let done = self.receiver().try_recv().ok()?;
        Some(self.keep(done))
    }

    /// The answer to the job sent, waiting for it; none when no job was sent.
    pub fn wait(&mut self) -> Option<A> {
        if !self.pending {
            return None;
        }
        let done = self
            .receiver()
            .recv()
            .expect("the worker runs while a job is pending");
        Some(self.keep(done))
    }

    fn receiver(&mut self) -> &Receiver<Done<J, A>> {
        self.done
            .get_mut()
            .expect("no thread panics holding the answers")
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
