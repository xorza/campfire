//! How the engine runs threads of its own: each a job of one list, started as a named worker
//! that its holder joins, with the lock and the failure a worker and its holder share
//! (02-engine-core.md, Threads are a crate of their own). It depends on no engine crate.

#![expect(
    clippy::disallowed_methods,
    reason = "the one crate that starts threads, which clippy.toml denies the others"
)]

mod failure;
mod job;
#[cfg(feature = "internals")]
mod parallel;
#[cfg(feature = "internals")]
mod race;
mod thread_name;
mod worker;
mod worker_lock;

pub use crate::failure::Failure;
pub use crate::job::Job;
pub use crate::thread_name::ThreadName;
pub use crate::worker::Worker;
pub use crate::worker_lock::WorkerLock;

/// What a test runs on threads of its own: jobs spread over the cores, and racers released at
/// once.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::parallel::Parallel;
    pub use crate::race::Race;
}
