use std::num::NonZero;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// A test's parallel run: independent jobs spread over as many threads as the machine has
/// cores, so a suite of slow cases fits its time bound.
#[derive(Debug)]
pub struct Parallel;

impl Parallel {
    /// What `run` gave for each index below `count`, by index. A thread takes the next index as
    /// it ends the last one, so a slow job holds no other back; a job's panic passes on.
    pub fn map<T: Send>(count: usize, run: impl Fn(usize) -> T + Sync) -> Vec<T> {
        Self::spread(count, &run)
    }

    fn spread<T: Send>(count: usize, run: &(dyn Fn(usize) -> T + Sync)) -> Vec<T> {
        let width = thread::available_parallelism()
            .map_or(1, NonZero::get)
            .min(count);
        let next = AtomicUsize::new(0);
        let mut done: Vec<(usize, T)> = thread::scope(|scope| {
            // Every thread spawns before the first join, or the jobs run one by one.
            let threads: Vec<_> = (0..width)
                .map(|_| {
                    scope.spawn(|| {
                        let mut done = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            if index >= count {
                                break done;
                            }
                            done.push((index, run(index)));
                        }
                    })
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
mod tests {
    use super::*;

    use std::sync::{Barrier, Mutex};

    /// Each index runs once and its result comes back in its place; no indices give nothing.
    #[test]
    fn each_index_runs_once_in_its_place() {
        assert_eq!(Parallel::map(0, |index| index), Vec::<usize>::new());
        assert_eq!(Parallel::map(1, |index| index * 10), [0]);
        let runs = Mutex::new(Vec::new());
        let squares = Parallel::map(100, |index| {
            runs.lock().unwrap().push(index);
            index * index
        });
        assert_eq!(
            squares,
            (0..100).map(|index| index * index).collect::<Vec<_>>()
        );
        let mut runs = runs.into_inner().unwrap();
        runs.sort_unstable();
        assert_eq!(runs, (0..100).collect::<Vec<_>>());
    }

    /// As many jobs as cores each wait until all of them run, so no thread takes a second job:
    /// they run on one thread of their own for each core, none the caller's. A pool narrower
    /// than the cores never releases the barrier, and the test ends at its bound.
    #[test]
    fn the_jobs_run_on_one_thread_for_each_core() {
        let cores = thread::available_parallelism().map_or(1, NonZero::get);
        let barrier = Barrier::new(cores);
        let threads = Parallel::map(cores, |_| {
            barrier.wait();
            thread::current().id()
        });
        assert!(!threads.contains(&thread::current().id()));
        for (index, id) in threads.iter().enumerate() {
            assert!(
                !threads[..index].contains(id),
                "job {index} shares a thread"
            );
        }
    }

    #[test]
    #[should_panic(expected = "job 3")]
    fn a_jobs_panic_passes_on() {
        Parallel::map(8, |index| assert_ne!(index, 3, "job 3"));
    }
}
