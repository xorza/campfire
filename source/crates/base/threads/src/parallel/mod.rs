use std::num::NonZero;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crate::thread_name::ThreadName;

/// A test's parallel run: independent jobs spread over as many threads as the machine has
/// cores, so a suite of slow cases fits its time bound.
#[derive(Debug)]
pub struct Parallel;

impl Parallel {
    /// The name of each of its threads, as a profiler shows it.
    const NAME: ThreadName = ThreadName::fixture("parallel");

    /// What `run` gave for each index below `count`, by index. A thread takes the next index as
    /// it ends the last one, so a slow job holds no other back; a job's panic passes on.
    pub fn map<T: Send>(count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
        Self::spread(count, &|| (), &|(), index| run(index))
    }

    /// As `map`, each thread with a state of its own that `init` makes as the thread takes its
    /// first index and `run` takes with each index, as rayon's `map_init`: a fixture too slow to
    /// build for each job is built once for each thread.
    pub fn map_init<S, T: Send>(
        count: usize,
        init: impl Fn() -> S + Sync,
        run: impl Fn(&mut S, usize) -> T + Sync,
    ) -> Vec<T> {
        Self::spread(count, &init, &run)
    }

    fn spread<S, T: Send>(
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
                    let builder = thread::Builder::new().name(Self::NAME.as_str().to_owned());
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
                    started.expect("a test's thread starts")
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

#[cfg(test)]
mod tests;
