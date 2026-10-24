use std::num::NonZero;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crate::job::Job;
use crate::thread_name::ThreadName;

/// A parallel run of independent jobs spread over as many threads as the machine has cores, each
/// result in its index's place, so the output is the same whatever thread ran a job: a cold path's
/// data to split, as an import's files, or a test's slow cases.
#[derive(Debug)]
pub struct Parallel;

impl Parallel {
    /// What `run` gave for each index below `count`, by index, its threads named after `job`.
    /// A thread takes the next index as it ends the last one, so a slow job holds no other back;
    /// a job's panic passes on.
    pub fn map_job<T: Send>(job: Job, count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
        Self::spread(job.into(), count, &|| (), &|(), index| run(index))
    }

    fn spread<S, T: Send>(
        name: ThreadName,
        count: usize,
        init: &(dyn Fn() -> S + Sync),
        run: &(dyn Fn(&mut S, usize) -> T + Sync),
    ) -> Vec<T> {
        let width = thread::available_parallelism()
            .map_or(1, NonZero::get)
            .min(count);
        let next = AtomicUsize::new(0);
        let mut done: Vec<(usize, T)> = thread::scope(|scope| {
            // Every thread spawns before the first join, or the jobs run one by one.
            let threads: Vec<_> = (0..width)
                .map(|_| {
                    let builder = thread::Builder::new().name(name.as_str().to_owned());
                    let started = builder.spawn_scoped(scope, || {
                        let mut done = Vec::new();
                        let mut state = None;
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            if index >= count {
                                break done;
                            }
                            let state = state.get_or_insert_with(init);
                            done.push((index, run(state, index)));
                        }
                    });
                    started.expect("a parallel run's thread starts")
                })
                .collect();
            threads
                .into_iter()
                .flat_map(|thread| {
                    thread
                        .join()
                        .unwrap_or_else(|payload| panic::resume_unwind(payload))
                })
                .collect()
        });
        done.sort_unstable_by_key(|&(index, _)| index);
        debug_assert!(done.iter().map(|&(index, _)| index).eq(0..count));
        done.into_iter().map(|(_, value)| value).collect()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::parallel::Parallel;
    use crate::thread_name::ThreadName;

    impl Parallel {
        /// The name of each of a test's threads, as a profiler shows it.
        const NAME: ThreadName = ThreadName::fixture("parallel");

        /// What `run` gave for each index below `count`, by index, as `map_job` gives it, on a
        /// test's threads, so a suite of slow cases fits its time bound.
        pub fn map<T: Send>(count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
            Self::spread(Self::NAME, count, &|| (), &|(), index| run(index))
        }

        /// As `map`, each thread with a state of its own that `init` makes as the thread takes
        /// its first index and `run` takes with each index, as rayon's `map_init`: a fixture too
        /// slow to build for each job is built once for each thread.
        pub fn map_init<S, T: Send>(
            count: usize,
            init: impl Fn() -> S + Sync,
            run: impl Fn(&mut S, usize) -> T + Sync,
        ) -> Vec<T> {
            Self::spread(Self::NAME, count, &init, &run)
        }
    }
}

#[cfg(test)]
mod tests;
